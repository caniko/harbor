//! Closed OpenLB PVD→VTM→VTI identity, captured before presentation can run.
use crate::{
    Error, Result,
    contracts::{ArtifactManifest, ExecutionPlan, digest, invalid},
    storage::{
        Store, commit_artifact, copy_verified, native_manifest, private_dir, safe_path,
        sync_directories,
    },
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    os::unix::fs::OpenOptionsExt,
    path::{Component, Path, PathBuf},
};

const COLLECTION: &str = "tmp/vtkData/channel.pvd";
const RECEIPT: &str = "openlb-receipt.json";
const XML_LIMIT: u64 = 64 * 1024 * 1024;
const MANIFEST_LIMIT: u64 = 2 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RetainedTime {
    pub requested_s: f64,
    pub observed_s: f64,
    pub step: u64,
    pub multiblock: String,
    pub shards: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FieldSnapshot {
    pub schema_version: u32,
    pub science_id: String,
    pub execution_id: String,
    pub execution_binding_digest: String,
    pub artifact_id: String,
    pub collection: String,
    pub times: Vec<RetainedTime>,
    pub files: Vec<ArtifactManifest>,
    pub field_units: serde_json::Value,
    pub physical_validation: String,
}

fn bytes(root: &Path, path: &str, maximum: u64) -> Result<Vec<u8>> {
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(safe_path(root, path)?)?;
    let size = file.metadata()?.len();
    if !file.metadata()?.is_file() || size == 0 || size > maximum {
        return Err(invalid("bounded closed field record required"));
    }
    let mut data = Vec::new();
    file.take(maximum + 1).read_to_end(&mut data)?;
    if data.len() as u64 != size {
        return Err(invalid("field record changed while reading"));
    }
    Ok(data)
}

fn document(text: &str) -> Result<roxmltree::Document<'_>> {
    // https://docs.rs/roxmltree/0.21.1/roxmltree/struct.ParsingOptions.html
    // Reject every DTD (including an empty one), with no external resolver.
    if text.contains("<!DOCTYPE") {
        return Err(invalid("VTK DTD rejected"));
    }
    roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 32768,
            ..Default::default()
        },
    )
    .map_err(|_| invalid("bounded well-formed VTK XML required"))
}

fn xml(root: &Path, path: &str, maximum: u64) -> Result<String> {
    String::from_utf8(bytes(root, path, maximum.min(XML_LIMIT))?)
        .map_err(|_| invalid("UTF-8 inline VTK XML required; appended raw arrays unsupported"))
}

fn reference(parent: &str, name: &str, extension: &str) -> Result<String> {
    let p = Path::new(name);
    if name.is_empty()
        || name.len() > 4096
        || p.components().any(|c| !matches!(c, Component::Normal(_)))
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/_-.".contains(&b))
        || p.extension().and_then(|x| x.to_str()) != Some(extension)
    {
        return Err(invalid(
            "safe relative VTK reference with exact supported extension required",
        ));
    }
    let path = Path::new(parent)
        .parent()
        .ok_or_else(|| invalid("collection parent"))?
        .join(p);
    let name = path.to_str().ok_or_else(|| invalid("VTK path encoding"))?;
    if !name.starts_with("tmp/vtkData/") {
        return Err(invalid("VTK reference leaves retained tree"));
    }
    Ok(name.into())
}

fn root_type<'a, 'b>(
    doc: &'a roxmltree::Document<'b>,
    kind: &str,
) -> Result<roxmltree::Node<'a, 'b>> {
    let root = doc.root_element();
    if !root.has_tag_name("VTKFile")
        || root.attribute("type") != Some(kind)
        || root
            .descendants()
            .any(|n| n.is_element() && n.tag_name().namespace().is_some())
    {
        return Err(invalid("unsupported VTK type or XML namespace"));
    }
    Ok(root)
}

fn image(root: &Path, path: &str, maximum: u64) -> Result<String> {
    let text = xml(root, path, maximum)?;
    let doc = document(&text)?;
    let vtk = root_type(&doc, "ImageData")?;
    let images: Vec<_> = vtk
        .children()
        .filter(|n| n.has_tag_name("ImageData"))
        .collect();
    if images.len() != 1 {
        return Err(invalid("one supported ImageData required"));
    }
    let pieces: Vec<_> = images[0]
        .children()
        .filter(|n| n.has_tag_name("Piece"))
        .collect();
    if pieces.len() != 1 || pieces[0].attribute("Extent") != images[0].attribute("WholeExtent") {
        return Err(invalid(
            "single complete ImageData piece required; topology conversion unsupported",
        ));
    }
    for attribute in ["Origin", "Spacing"] {
        let values: Vec<_> = images[0]
            .attribute(attribute)
            .unwrap_or("")
            .split_whitespace()
            .map(str::parse::<f64>)
            .collect::<std::result::Result<_, _>>()
            .map_err(|_| invalid("VTK coordinates"))?;
        if values.len() != 3
            || values
                .iter()
                .any(|v| !v.is_finite() || (attribute == "Spacing" && *v <= 0.))
        {
            return Err(invalid(
                "finite 3D VTK origin and positive spacing required",
            ));
        }
    }
    let extent: Vec<_> = images[0]
        .attribute("WholeExtent")
        .unwrap_or("")
        .split_whitespace()
        .map(str::parse::<i64>)
        .collect::<std::result::Result<_, _>>()
        .map_err(|_| invalid("VTK extent"))?;
    if extent.len() != 6 || extent.as_chunks::<2>().0.iter().any(|p| p[0] > p[1]) {
        return Err(invalid("ordered 3D VTK extent required"));
    }
    let mut names = BTreeSet::new();
    for array in vtk.descendants().filter(|n| n.has_tag_name("DataArray")) {
        let name = array.attribute("Name").unwrap_or("");
        let components = match name {
            "physVelocity" => "3",
            "physPressure" | "geometry" => "1",
            _ => return Err(invalid("unsupported scientific VTI array")),
        };
        if !array.parent().is_some_and(|n| n.has_tag_name("PointData"))
            || array.attribute("type") != Some("Float64")
            || array.attribute("NumberOfComponents") != Some(components)
            || !names.insert(name)
            || array.attribute("format") != Some("binary")
            || array.attribute("encoding") != Some("base64")
        {
            return Err(invalid(
                "exact Float64 point-associated native arrays required",
            ));
        }
    }
    if names != BTreeSet::from(["physVelocity", "physPressure", "geometry"])
        || vtk
            .descendants()
            .any(|n| n.attribute("file").is_some() || n.has_tag_name("AppendedData"))
    {
        return Err(invalid("complete inline scientific VTI required"));
    }
    Ok(format!("{:x}", Sha256::digest(text.as_bytes())))
}

/// Capture only the supported collection graph; no recompute or format conversion.
pub fn capture(
    source: &Path,
    destination: &Path,
    plan: &ExecutionPlan,
    binding_digest: &str,
) -> Result<FieldSnapshot> {
    if destination.exists() {
        return Err(invalid("immutable field snapshot already exists"));
    }
    let maximum = plan.observation.max_artifact_bytes;
    let receipt_bytes = bytes(source, RECEIPT, maximum.min(2 * 1024 * 1024))?;
    let receipt: serde_json::Value = serde_json::from_slice(&receipt_bytes)?;
    let mut parsed_hashes = BTreeMap::from([(
        RECEIPT.to_owned(),
        format!("{:x}", Sha256::digest(&receipt_bytes)),
    )]);
    let retained = receipt["retained_times"]
        .as_array()
        .ok_or_else(|| invalid("retained time receipt missing"))?;
    let expected = &plan.observation.retained_times_s;
    plan.validate()?;
    // Same operation order as the pinned OpenLB converter and approval gate.
    let case = plan.channel_case()?;
    let dx = case.channel_height.si("length")? / f64::from(case.resolution);
    let dt =
        ((0.8f64 - 0.5) / 3.) * (dx * dx) / case.kinematic_viscosity.si("kinematic_viscosity")?;
    if retained.len() != expected.len()
        || expected.is_empty()
        || receipt["executed"] != true
        || receipt["software_fallback"] != false
        || receipt["physical_step_s"].as_f64() != Some(dt)
        || receipt["field_units"]
            != serde_json::json!({"geometry":"material ID","physPressure":"Pa","physVelocity":"m/s"})
    {
        return Err(invalid(
            "completed solver fields and exact approved retained times/units required",
        ));
    }
    let text = xml(source, COLLECTION, maximum.min(2 * 1024 * 1024))?;
    parsed_hashes.insert(
        COLLECTION.into(),
        format!("{:x}", Sha256::digest(text.as_bytes())),
    );
    let doc = document(&text)?;
    let vtk = root_type(&doc, "Collection")?;
    let datasets: Vec<_> = vtk
        .descendants()
        .filter(|n| n.has_tag_name("DataSet"))
        .collect();
    if datasets.len() != retained.len()
        || datasets
            .iter()
            .any(|n| !n.parent().is_some_and(|p| p.has_tag_name("Collection")))
    {
        return Err(invalid(
            "one supported PVD dataset for each retained time required",
        ));
    }
    let mut paths = BTreeSet::from([COLLECTION.to_owned(), RECEIPT.to_owned()]);
    let mut times = Vec::new();
    let mut previous = None;
    for ((dataset, receipt), requested) in datasets.iter().zip(retained).zip(expected) {
        let step = receipt["step"]
            .as_u64()
            .ok_or_else(|| invalid("retained lattice step"))?;
        let observed = receipt["observed_s"]
            .as_f64()
            .filter(|v| v.is_finite() && *v >= 0.)
            .ok_or_else(|| invalid("finite observed physical time required"))?;
        if receipt["requested_s"].as_f64() != Some(*requested)
            || step != (*requested / dt + 0.5).floor() as u64
            || (observed - step as f64 * dt).abs() > 16. * f64::EPSILON * observed.abs().max(1.)
            || dataset
                .attribute("timestep")
                .and_then(|s| s.parse::<u64>().ok())
                != Some(step)
            || previous.is_some_and(|p| p >= step)
            || !matches!(dataset.attribute("part"), None | Some("") | Some("0"))
            || !matches!(dataset.attribute("group"), None | Some(""))
        {
            return Err(invalid(
                "PVD time/partition differs from exact retained solver state",
            ));
        }
        previous = Some(step);
        let multiblock = reference(COLLECTION, dataset.attribute("file").unwrap_or(""), "vtm")?;
        if !paths.insert(multiblock.clone()) {
            return Err(invalid("duplicate retained multiblock"));
        }
        let text = xml(source, &multiblock, maximum.min(2 * 1024 * 1024))?;
        parsed_hashes.insert(
            multiblock.clone(),
            format!("{:x}", Sha256::digest(text.as_bytes())),
        );
        let doc = document(&text)?;
        let vtk = root_type(&doc, "vtkMultiBlockDataSet")?;
        let blocks: Vec<_> = vtk
            .descendants()
            .filter(|n| n.has_tag_name("DataSet"))
            .collect();
        if blocks.len() != 1 {
            return Err(Error::Unqualified("current field snapshot supports one native VTI shard; partition/ghost-cell round trips unqualified".into()));
        }
        if blocks[0].attribute("index") != Some("0")
            || !blocks[0].parent().is_some_and(|n| {
                n.has_tag_name("Block")
                    && n.attribute("index") == Some("0")
                    && n.parent().is_some_and(|p| {
                        p.has_tag_name("vtkMultiBlockDataSet") && p.parent() == Some(vtk)
                    })
            })
        {
            return Err(invalid("exact single-block native VTM structure required"));
        }
        let shard = reference(
            &multiblock,
            blocks[0].attribute("file").unwrap_or(""),
            "vti",
        )?;
        if !paths.insert(shard.clone()) {
            return Err(invalid("duplicate retained VTI shard"));
        }
        parsed_hashes.insert(shard.clone(), image(source, &shard, maximum)?);
        times.push(RetainedTime {
            requested_s: *requested,
            observed_s: observed,
            step,
            multiblock,
            shards: vec![shard],
        });
    }
    let mut files = Vec::new();
    let mut total = 0u64;
    for path in paths {
        let record = native_manifest(
            source,
            &path,
            maximum,
            "immutable retained solver fields; captured before independent presentation",
        )?;
        if parsed_hashes.get(&path) != Some(&record.sha256) {
            return Err(invalid("scientific collection changed after parsing"));
        }
        total = total
            .checked_add(record.bytes)
            .ok_or_else(|| invalid("snapshot byte overflow"))?;
        if total > maximum {
            return Err(Error::Resource(
                "retained field snapshot budget exhausted".into(),
            ));
        }
        files.push(record);
    }
    let snapshot = FieldSnapshot {
        schema_version: 1,
        science_id: plan.science_id()?,
        execution_id: plan.id()?,
        execution_binding_digest: binding_digest.into(),
        artifact_id: digest(&files)?,
        collection: COLLECTION.into(),
        times,
        files,
        field_units: receipt["field_units"].clone(),
        physical_validation: "unqualified".into(),
    };
    let snapshot_bytes = serde_json::to_vec_pretty(&snapshot)?;
    if snapshot_bytes.len() > 2 * 1024 * 1024
        || total
            .checked_add(snapshot_bytes.len() as u64)
            .is_none_or(|n| n > maximum)
    {
        return Err(Error::Resource(
            "bounded field snapshot manifest budget exhausted".into(),
        ));
    }
    let staging = destination
        .parent()
        .ok_or_else(|| invalid("snapshot parent"))?
        .join(format!(".fields-incomplete-{}", uuid::Uuid::new_v4()));
    private_dir(&staging)?;
    let result = (|| {
        for record in &snapshot.files {
            copy_verified(source, &staging, record)?;
        }
        commit_artifact(
            &staging,
            "snapshot.json",
            &snapshot_bytes,
            "json",
            "immutable science/artifact/time binding before presentation",
        )?;
        verify(&staging, &snapshot)?;
        sync_directories(&staging)?;
        // Publish without replacing an existing immutable destination.
        crate::storage::publish_directory(&staging, destination)?;
        fs::File::open(
            destination
                .parent()
                .ok_or_else(|| invalid("snapshot parent"))?,
        )?
        .sync_all()?;
        Ok(snapshot)
    })();
    if staging.exists() {
        let _ = fs::remove_dir_all(staging);
    }
    result
}

pub fn verify(root: &Path, snapshot: &FieldSnapshot) -> Result<()> {
    if snapshot.schema_version != 1 || snapshot.artifact_id != digest(&snapshot.files)? {
        return Err(invalid("field snapshot identity mismatch"));
    }
    for expected in &snapshot.files {
        let observed = native_manifest(root, &expected.path, expected.bytes, "verify")?;
        if expected.sha256 != observed.sha256 || expected.bytes != observed.bytes {
            return Err(invalid("retained scientific bytes changed"));
        }
    }
    Ok(())
}

/// A self-consistent manifest cannot replace the descriptor committed by the worker.
pub(crate) fn registered(store: &Store, id: &str) -> Result<(PathBuf, FieldSnapshot, String)> {
    let root = safe_path(&store.job_dir(id)?, "retained-fields")?;
    let manifest = store
        .artifacts(id)?
        .into_iter()
        .find(|m| m.path == "retained-fields/snapshot.json")
        .ok_or_else(|| invalid("committed field snapshot descriptor required"))?;
    let data = bytes(&root, "snapshot.json", MANIFEST_LIMIT)?;
    let observed = format!("{:x}", Sha256::digest(&data));
    if manifest.bytes != data.len() as u64 || manifest.sha256 != observed {
        return Err(invalid("committed field snapshot bytes changed"));
    }
    let snapshot: FieldSnapshot = serde_json::from_slice(&data)?;
    verify(&root, &snapshot)?;
    Ok((root, snapshot, observed))
}

pub(crate) fn manifests(root: &Path, snapshot: &FieldSnapshot) -> Result<Vec<ArtifactManifest>> {
    let mut records = snapshot.files.clone();
    records.push(native_manifest(
        root,
        "snapshot.json",
        2 * 1024 * 1024,
        "immutable science/artifact/time binding before presentation",
    )?);
    for record in &mut records {
        record.path = format!("retained-fields/{}", record.path);
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registered_snapshot_rejects_self_consistent_metadata_substitution() {
        use crate::{contracts::CaseSpec, storage::Store};
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("state")).unwrap();
        let plan = ExecutionPlan::reference(CaseSpec::reference()).unwrap();
        let profile = crate::worker::load_profile(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/profiles/ci.json"
        )))
        .unwrap();
        let job = store
            .submit_with_profile(&plan, "committed-snapshot", &profile)
            .unwrap();
        let root = store.job_dir(&job.id).unwrap().join("retained-fields");
        private_dir(&root).unwrap();
        // Descriptor trust is independent of the collection parser exercised by
        // the native-field tests. Even self-consistent metadata must match it.
        let mut snapshot = FieldSnapshot {
            schema_version: 1,
            science_id: plan.science_id().unwrap(),
            execution_id: plan.id().unwrap(),
            execution_binding_digest: "binding".into(),
            artifact_id: digest(&Vec::<ArtifactManifest>::new()).unwrap(),
            collection: COLLECTION.into(),
            times: vec![],
            files: vec![],
            field_units: serde_json::json!({}),
            physical_validation: "unqualified".into(),
        };
        commit_artifact(
            &root,
            "snapshot.json",
            &serde_json::to_vec(&snapshot).unwrap(),
            "json",
            "fixture",
        )
        .unwrap();
        store
            .add_artifacts(&job.id, &manifests(&root, &snapshot).unwrap())
            .unwrap();
        assert!(registered(&store, &job.id).is_ok());
        snapshot.science_id = "substituted-science".into();
        fs::write(
            root.join("snapshot.json"),
            serde_json::to_vec(&snapshot).unwrap(),
        )
        .unwrap();
        assert!(verify(&root, &snapshot).is_ok());
        assert!(registered(&store, &job.id).is_err());
    }

    #[test]
    fn artifact_batch_failure_rolls_back_every_descriptor() {
        use crate::{contracts::CaseSpec, storage::Store};
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("state")).unwrap();
        let plan = ExecutionPlan::reference(CaseSpec::reference()).unwrap();
        let profile = crate::worker::load_profile(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/profiles/ci.json"
        )))
        .unwrap();
        let job = store
            .submit_with_profile(&plan, "atomic-records", &profile)
            .unwrap();
        let record = commit_artifact(
            &store.job_dir(&job.id).unwrap(),
            "record.json",
            b"{}",
            "json",
            "fixture",
        )
        .unwrap();
        assert!(
            store
                .add_artifacts(&job.id, &[record.clone(), record])
                .is_err()
        );
        assert!(store.artifacts(&job.id).unwrap().is_empty());
    }
}
