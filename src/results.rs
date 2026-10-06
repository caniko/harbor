//! Bounded read-only samples of registered static native FEM; no interpolation.
use crate::{
    Error, Result,
    contracts::*,
    qualification::{self, EvidenceRecord, EvidenceState},
    storage::{Store, safe_path},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

const FIELD_LIMIT: u64 = 32 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StaticField {
    Temperature,
    HeatFlux,
    Displacement,
    Stress,
}
impl StaticField {
    fn metadata(&self) -> (&'static str, &'static str, usize, bool) {
        match self {
            Self::Temperature => ("temperature", "K", 1, true),
            Self::HeatFlux => ("heat_flux", "W/m2", 3, false),
            Self::Displacement => ("displacement", "m", 3, true),
            Self::Stress => ("stress", "Pa", 6, false),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "association", rename_all = "snake_case", deny_unknown_fields)]
pub enum SampleLocation {
    Node {
        node_id: u64,
    },
    IntegrationPoint {
        element_id: u64,
        integration_point: u8,
    },
}
impl SampleLocation {
    fn identity(&self) -> Vec<u64> {
        match self {
            Self::Node { node_id } => vec![*node_id],
            Self::IntegrationPoint {
                element_id,
                integration_point,
            } => vec![*element_id, u64::from(*integration_point)],
        }
    }
    fn validate(&self, nodal: bool) -> bool {
        match self {
            Self::Node { node_id } => nodal && *node_id > 0,
            Self::IntegrationPoint {
                element_id,
                integration_point,
            } => !nodal && *element_id > 0 && (1..=8).contains(integration_point),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SampleRequest {
    pub schema_version: u32,
    pub job_id: String,
    pub field: StaticField,
    pub locations: Vec<SampleLocation>,
}
impl SampleRequest {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1
            || uuid::Uuid::parse_str(&self.job_id).is_err()
            || self.locations.is_empty()
            || self.locations.len() > 64
            || self.locations.iter().collect::<BTreeSet<_>>().len() != self.locations.len()
            || self
                .locations
                .iter()
                .any(|v| !v.validate(self.field.metadata().3))
        {
            return Err(invalid(
                "versioned static field with 1–64 distinct typed native locations required",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompareRequest {
    pub schema_version: u32,
    pub left: SampleRequest,
    pub right: SampleRequest,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SampleValue {
    pub location: SampleLocation,
    pub value: Vec<f64>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SampleReport {
    pub schema_version: u32,
    pub job_id: String,
    pub science_id: String,
    pub execution_id: String,
    pub execution_binding_digest: String,
    pub field: StaticField,
    pub unit: String,
    pub components: usize,
    pub coordinate_unit: String,
    pub physical_time_s: Option<f64>,
    pub field_artifact: EvidenceRecord,
    pub native_artifact: EvidenceRecord,
    pub mesh_artifact: EvidenceRecord,
    pub source_samples: usize,
    pub samples: Vec<SampleValue>,
    pub interpolation: String,
    pub physical_validation: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompareReport {
    pub schema_version: u32,
    pub left: SampleReport,
    pub right: SampleReport,
    pub differences: Vec<SampleValue>,
    pub maximum_abs_difference: f64,
    pub numerical_acceptance: String,
    pub physical_validation: String,
}

fn registered(store: &Store, id: &str, path: &str) -> Result<(EvidenceRecord, serde_json::Value)> {
    let (record, data) = registered_bytes(store, id, path, "json")?;
    Ok((record, serde_json::from_slice(&data)?))
}

fn registered_bytes(
    store: &Store,
    id: &str,
    path: &str,
    format: &str,
) -> Result<(EvidenceRecord, Vec<u8>)> {
    let record = store
        .artifact_record(id, path)?
        .ok_or_else(|| Error::Unqualified("registered native result missing".into()))?;
    if record.path != path
        || record.format != format
        || record.bytes == 0
        || record.bytes > FIELD_LIMIT
    {
        return Err(invalid("bounded registered native result object required"));
    }
    let data = crate::worker::read_bounded(&safe_path(&store.job_dir(id)?, path)?, FIELD_LIMIT)?;
    if data.len() as u64 != record.bytes || format!("{:x}", Sha256::digest(&data)) != record.sha256
    {
        return Err(invalid("native result differs from registered bytes"));
    }
    Ok((
        EvidenceRecord {
            path: path.into(),
            sha256: record.sha256,
            bytes: record.bytes,
        },
        data,
    ))
}

fn native_field(field: &StaticField, text: &str) -> Result<serde_json::Value> {
    let (name, _, components, nodal) = field.metadata();
    let label = match field {
        StaticField::Temperature => "temperatures",
        StaticField::HeatFlux => "heat flux (elem, integ.pnt.,qx,qy,qz)",
        StaticField::Displacement => "displacements (vx,vy,vz)",
        StaticField::Stress => "stresses (elem, integ.pnt.,sxx,syy,szz,sxy,sxz,syz)",
    };
    let expected_set = if nodal { "NALL" } else { "EALL" };
    let ids = if nodal { 1 } else { 2 };
    let mut observed = false;
    let mut active = false;
    let mut values = BTreeMap::new();
    for line in text.lines().map(str::trim).filter(|s| !s.is_empty()) {
        if let Some((section, rest)) = line.split_once(" for set ") {
            active = section == label;
            if active {
                let header: Vec<_> = rest.split_whitespace().collect();
                if observed
                    || header.len() != 4
                    || header[0] != expected_set
                    || header[1..3] != ["and", "time"]
                    || header[3].replace('D', "E").parse::<f64>().ok() != Some(1.)
                {
                    return Err(invalid(
                        "one final native static section and exact entity set required",
                    ));
                }
                observed = true;
            }
        } else if line.starts_with("S T E P ") || line.starts_with("INCREMENT ") {
            active = false;
        } else if active {
            let row: Vec<_> = line.split_whitespace().collect();
            if row.len() != ids + components {
                return Err(invalid("native static component coverage changed"));
            }
            let identity = row[..ids]
                .iter()
                .map(|v| {
                    v.parse::<u64>()
                        .ok()
                        .filter(|i| *i > 0)
                        .ok_or_else(|| invalid("positive native static ID required"))
                })
                .collect::<Result<Vec<_>>>()?;
            let value = row[ids..]
                .iter()
                .map(|v| {
                    v.replace('D', "E")
                        .parse::<f64>()
                        .ok()
                        .filter(|v| v.is_finite())
                        .ok_or_else(|| invalid("finite native static component required"))
                })
                .collect::<Result<Vec<_>>>()?;
            if values.insert(identity, value).is_some() {
                return Err(invalid("duplicate authoritative native static ID"));
            }
        }
    }
    if !observed {
        return Err(invalid("authoritative native static field absent"));
    }
    let records: Vec<_> = values
        .into_iter()
        .map(|(id, value)| serde_json::json!({"id":id,"value":value}))
        .collect();
    Ok(
        serde_json::json!({"schema_version":1,"static":true,"coordinate_unit":"m","fields":{name:[{"solver_step_parameter":1.,"physical_time_s":null,"values":records}]}}),
    )
}

fn extract(
    request: &SampleRequest,
    mesh: &serde_json::Value,
    fields: &serde_json::Value,
    resolution: u32,
) -> Result<(usize, Vec<SampleValue>)> {
    request.validate()?;
    let (name, _, components, nodal) = request.field.metadata();
    if fields["schema_version"] != 1 || fields["static"] != true || fields["coordinate_unit"] != "m"
    {
        return Err(invalid("explicit static SI native result required"));
    }
    let snapshots = fields["fields"][name]
        .as_array()
        .filter(|v| v.len() == 1)
        .ok_or_else(|| invalid("one complete static snapshot of requested field required"))?;
    if snapshots[0].get("physical_time_s") != Some(&serde_json::Value::Null)
        || snapshots[0]["solver_step_parameter"]
            .as_f64()
            .is_none_or(|v| !v.is_finite())
    {
        return Err(invalid(
            "static solver parameter must not invent physical time",
        ));
    }
    let records = snapshots[0]["values"]
        .as_array()
        .ok_or_else(|| invalid("explicit native field samples required"))?;
    let count = if nodal {
        (resolution as usize + 1).pow(3)
    } else {
        8 * (resolution as usize).pow(3)
    };
    let entities = mesh[if nodal { "nodes" } else { "elements" }]
        .as_object()
        .ok_or_else(|| invalid("native mesh identity table required"))?;
    if records.len() != count || entities.len() != if nodal { count } else { count / 8 } {
        return Err(invalid("complete native static coverage required"));
    }
    let requested: BTreeMap<_, _> = request
        .locations
        .iter()
        .map(|v| (v.identity(), v))
        .collect();
    let mut seen = BTreeSet::new();
    let mut selected = BTreeMap::new();
    for record in records {
        let identity = record["id"]
            .as_array()
            .ok_or_else(|| invalid("native entity IDs required"))?
            .iter()
            .map(|v| {
                v.as_u64()
                    .ok_or_else(|| invalid("integer native entity identity required"))
            })
            .collect::<Result<Vec<_>>>()?;
        if identity.len() != if nodal { 1 } else { 2 }
            || !entities.contains_key(&identity[0].to_string())
            || (!nodal && !(1..=8).contains(&identity[1]))
            || !seen.insert(identity.clone())
        {
            return Err(invalid("native field entity/association coverage changed"));
        }
        let value = record["value"]
            .as_array()
            .ok_or_else(|| invalid("explicit native components required"))?
            .iter()
            .map(|v| {
                v.as_f64()
                    .filter(|v| v.is_finite())
                    .ok_or_else(|| invalid("finite native component required"))
            })
            .collect::<Result<Vec<_>>>()?;
        if value.len() != components {
            return Err(invalid("native field components changed"));
        }
        if requested.contains_key(&identity) {
            selected.insert(identity, value);
        }
    }
    let samples = request
        .locations
        .iter()
        .map(|location| {
            Ok(SampleValue {
                location: location.clone(),
                value: selected
                    .remove(&location.identity())
                    .ok_or_else(|| invalid("requested native entity absent"))?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((count, samples))
}

pub fn sample(store: &Store, request: &SampleRequest) -> Result<SampleReport> {
    request.validate()?;
    let plan = store.plan(&request.job_id)?;
    let (stage, field_path, mesh_path, native_path, spec) = if let Some(spec) = &plan.imported_fem {
        (
            StageOperation::FemImported,
            "stages/fem-imported/imported-fields.json",
            "stages/fem-imported/mesh.json",
            "stages/fem-imported/reference.dat",
            &spec.reference,
        )
    } else if let Some(spec) = &plan.fem {
        (
            StageOperation::FemReference,
            "stages/fem/fields.json",
            "stages/fem/mesh.json",
            "stages/fem/reference.dat",
            spec,
        )
    } else {
        return Err(Error::Unqualified(
            "sampling supports registered static native FEM only".into(),
        ));
    };
    let evidence = qualification::inspect(store, &request.job_id)?;
    if evidence.job_state != "succeeded"
        || !evidence.capabilities.iter().any(|v| {
            v.operation == stage
                && matches!(v.runtime_execution, EvidenceState::Recorded)
                && matches!(v.numerical_verification, EvidenceState::ReportedPass)
        })
    {
        return Err(Error::Unqualified(
            "succeeded source-bound native numerical result required".into(),
        ));
    }
    let (field_artifact, fields) = registered(store, &request.job_id, field_path)?;
    let (mesh_artifact, mesh) = registered(store, &request.job_id, mesh_path)?;
    let (source_samples, samples) = extract(request, &mesh, &fields, spec.resolution)?;
    let (native_artifact, data) = registered_bytes(store, &request.job_id, native_path, "dat")?;
    let native = native_field(
        &request.field,
        std::str::from_utf8(&data).map_err(|_| invalid("native static output must be text"))?,
    )?;
    if fields["fields"][request.field.metadata().0] != native["fields"][request.field.metadata().0]
    {
        return Err(invalid(
            "registered static fields differ from authoritative native output",
        ));
    }
    let (_, unit, components, _) = request.field.metadata();
    Ok(SampleReport {
        schema_version: 1,
        job_id: request.job_id.clone(),
        science_id: evidence.science_id,
        execution_id: evidence.execution_id,
        execution_binding_digest: digest(
            &evidence
                .execution_binding
                .ok_or_else(|| Error::Unqualified("native execution binding required".into()))?,
        )?,
        field: request.field.clone(),
        unit: unit.into(),
        components,
        coordinate_unit: "m".into(),
        physical_time_s: None,
        field_artifact,
        native_artifact,
        mesh_artifact,
        source_samples,
        samples,
        interpolation: "none; exact native entity identity".into(),
        physical_validation: "unqualified".into(),
    })
}
pub fn compare(store: &Store, request: &CompareRequest) -> Result<CompareReport> {
    request.left.validate()?;
    request.right.validate()?;
    if request.schema_version != 1
        || request.left.field != request.right.field
        || request.left.locations != request.right.locations
    {
        return Err(invalid(
            "same field and ordered native locations required for comparison",
        ));
    }
    compare_reports(
        sample(store, &request.left)?,
        sample(store, &request.right)?,
    )
}
fn compare_reports(left: SampleReport, right: SampleReport) -> Result<CompareReport> {
    if left.mesh_artifact.sha256 != right.mesh_artifact.sha256
        || left.field != right.field
        || left.unit != right.unit
        || left.components != right.components
        || left.samples.len() != right.samples.len()
    {
        return Err(invalid(
            "exact same native mesh/field/units/components required; no implicit registration",
        ));
    }
    let mut maximum_abs_difference: f64 = 0.;
    let mut differences = Vec::new();
    for (a, b) in left.samples.iter().zip(&right.samples) {
        if a.location != b.location {
            return Err(invalid("comparison entity identity changed"));
        }
        let value = a
            .value
            .iter()
            .zip(&b.value)
            .map(|(a, b)| {
                let delta = b - a;
                if !delta.is_finite() {
                    return Err(invalid("nonfinite result difference"));
                }
                maximum_abs_difference = maximum_abs_difference.max(delta.abs());
                Ok(delta)
            })
            .collect::<Result<Vec<_>>>()?;
        differences.push(SampleValue {
            location: a.location.clone(),
            value,
        });
    }
    Ok(CompareReport {
        schema_version: 1,
        left,
        right,
        differences,
        maximum_abs_difference,
        numerical_acceptance: "not assessed; right minus left values only".into(),
        physical_validation: "unqualified".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (SampleRequest, serde_json::Value, serde_json::Value) {
        let request = SampleRequest {
            schema_version: 1,
            job_id: uuid::Uuid::new_v4().to_string(),
            field: StaticField::Temperature,
            locations: vec![
                SampleLocation::Node { node_id: 8 },
                SampleLocation::Node { node_id: 1 },
            ],
        };
        let mesh = serde_json::json!({"nodes":(1..=8).map(|id|(id.to_string(),serde_json::json!([id,0,0]))).collect::<BTreeMap<_,_>>()});
        let fields = serde_json::json!({"schema_version":1,"static":true,"coordinate_unit":"m","fields":{"temperature":[{"physical_time_s":null,"solver_step_parameter":1,"values":(1..=8).map(|id|serde_json::json!({"id":[id],"value":[293.+id as f64]})).collect::<Vec<_>>()}]}});
        (request, mesh, fields)
    }
    #[test]
    fn exact_native_ids_keep_requested_order_and_static_association() {
        let (request, mesh, fields) = fixture();
        let (count, samples) = extract(&request, &mesh, &fields, 1).unwrap();
        assert_eq!(count, 8);
        assert_eq!(samples[0].value, vec![301.]);
        assert_eq!(samples[1].value, vec![294.]);
    }
    #[test]
    fn changed_native_coverage_components_times_or_mesh_ids_reject() {
        let (request, mesh, fields) = fixture();
        for change in [
            serde_json::json!({"id":[1],"value":[1,2]}),
            serde_json::json!({"id":[2],"value":[1]}),
            serde_json::json!({"id":[99],"value":[1]}),
        ] {
            let mut value = fields.clone();
            value["fields"]["temperature"][0]["values"][0] = change;
            assert!(extract(&request, &mesh, &value, 1).is_err());
        }
        let mut value = fields.clone();
        value["fields"]["temperature"][0]["physical_time_s"] = serde_json::json!(1.);
        assert!(extract(&request, &mesh, &value, 1).is_err());
        let mut changed = request.clone();
        changed.locations.push(changed.locations[0].clone());
        assert!(changed.validate().is_err());
        changed = request.clone();
        changed.field = StaticField::Stress;
        assert!(changed.validate().is_err());
    }
    #[test]
    fn comparison_reports_signed_differences_and_rejects_geometry_or_unit_drift() {
        let (request, mesh, fields) = fixture();
        let (_, samples) = extract(&request, &mesh, &fields, 1).unwrap();
        let left = SampleReport {
            schema_version: 1,
            job_id: request.job_id,
            science_id: "a".repeat(64),
            execution_id: "b".repeat(64),
            execution_binding_digest: "c".repeat(64),
            field: request.field,
            unit: "K".into(),
            components: 1,
            coordinate_unit: "m".into(),
            physical_time_s: None,
            field_artifact: EvidenceRecord {
                path: "fields.json".into(),
                sha256: "d".repeat(64),
                bytes: 100,
            },
            mesh_artifact: EvidenceRecord {
                path: "mesh.json".into(),
                sha256: "e".repeat(64),
                bytes: 100,
            },
            native_artifact: EvidenceRecord {
                path: "reference.dat".into(),
                sha256: "d".repeat(64),
                bytes: 100,
            },
            source_samples: 8,
            samples,
            interpolation: "none".into(),
            physical_validation: "unqualified".into(),
        };
        let mut right = left.clone();
        right.samples[0].value[0] -= 2.;
        right.samples[1].value[0] += 3.;
        let compared = compare_reports(left.clone(), right.clone()).unwrap();
        assert_eq!(compared.differences[0].value, vec![-2.]);
        assert_eq!(compared.differences[1].value, vec![3.]);
        assert_eq!(compared.maximum_abs_difference, 3.);
        right.unit = "degC".into();
        assert!(compare_reports(left.clone(), right.clone()).is_err());
        right.unit = "K".into();
        right.mesh_artifact.sha256 = "f".repeat(64);
        assert!(compare_reports(left, right).is_err());
    }

    #[test]
    fn native_static_samples_match_authoritative_fortran_dat_with_no_duplicate_sections() {
        let (request, mesh, fields) = fixture();
        let mut native =
            "S T E P 1\nINCREMENT 1\n\n temperatures for set NALL and time 1.000000D+00\n"
                .to_owned();
        for id in 1..=8 {
            native.push_str(&format!("{id} {}D+00\n", 293 + id));
        }
        let (_, expected) = extract(&request, &mesh, &fields, 1).unwrap();
        let (_, observed) = extract(
            &request,
            &mesh,
            &native_field(&request.field, &native).unwrap(),
            1,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(observed).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
        for changed in [
            native.replace("NALL", "FOREIGN"),
            format!("{native}{native}"),
            native.replace("1.000000D+00", "2.000000D+00"),
            native.replace("8 301", "8 nan"),
        ] {
            assert!(
                native_field(&request.field, &changed)
                    .and_then(|v| extract(&request, &mesh, &v, 1))
                    .is_err()
            );
        }
    }
}
