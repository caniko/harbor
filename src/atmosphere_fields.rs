//! Source-bound independent verification of retained original libRadtran fields.
use crate::{
    Result,
    atmosphere::*,
    contracts::{digest, invalid},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::Path;

fn same(actual: &Value, expected: &Value) -> bool {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => a.as_f64().zip(b.as_f64()).is_some_and(|(a, b)| {
            a.is_finite() && b.is_finite() && (a - b).abs() <= 1e-11 * a.abs().max(b.abs()) + 1e-15
        }),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len() && b.iter().all(|(k, b)| a.get(k).is_some_and(|a| same(a, b)))
        }
        _ => actual == expected,
    }
}

pub(crate) fn verify(
    spec: &AtmosphericReferenceSpec,
    root: &Path,
    receipt: &Value,
) -> Result<crate::qualification::NumericalEvidence> {
    let prepared = spec.prepare()?;
    if receipt["schema_version"] != 1
        || receipt["adapter"] != "libRadtran"
        || receipt["version"] != "2.0.6"
        || receipt["source_sha256"] != SOURCE_SHA256
        || receipt["profile_sha256"] != profile_sha256(&spec.profile)?
        || receipt["backend"] != "cpu"
        || receipt["solver"] != "disort"
        || receipt["precision"] != "Float32"
        || receipt["reduction_precision"] != "Float64"
        || receipt["executed"] != true
        || receipt["software_fallback"] != false
        || receipt["input"] != serde_json::to_value(spec)?
        || receipt["request_sha256"] != digest(spec)?
        || receipt["physical_validation"] != "unqualified"
    {
        return Err(invalid(
            "exact executed native atmospheric source/profile/request/ABI required",
        ));
    }
    let executable = receipt["native_executable"]
        .as_str()
        .filter(|s| s.ends_with("/bin/uvspec"))
        .ok_or_else(|| {
            invalid("exact immutable native atmospheric executable identity required")
        })?;
    crate::retention::store_object(executable)?;
    if !receipt["native_executable_sha256"]
        .as_str()
        .is_some_and(|s| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
    {
        return Err(invalid("original native executable SHA-256 required"));
    }
    let canaries = [
        "operation_closure_only",
        "no_gpu_nodes",
        "no_sysfs",
        "no_host_home",
        "no_session_bus",
        "no_worker_socket",
        "network_namespace_isolated",
        "descriptor_readonly",
    ];
    if receipt["sandbox"]["policy"] != SANDBOX_POLICY
        || !receipt["sandbox"]["checks"].as_object().is_some_and(|o| {
            o.len() == canaries.len()
                && canaries
                    .iter()
                    .all(|k| o.get(*k) == Some(&Value::Bool(true)))
        })
    {
        return Err(invalid(
            "complete atmosphere-only CPU isolation evidence required",
        ));
    }
    let mut normalized = serde_json::to_value(&prepared)?;
    normalized
        .as_object_mut()
        .ok_or_else(|| invalid("atmospheric normalization object"))?
        .remove("request_sha256");
    if !same(&receipt["prepared"], &normalized) {
        return Err(invalid(
            "approved atmospheric SI/source/angular preparation changed",
        ));
    }
    let original = crate::worker::read_bounded(
        &crate::storage::safe_path(root, "uvspec-original.txt")?,
        64 * 1024 * 1024,
    )?;
    let expected_original = serde_json::json!({"path":"uvspec-original.txt","bytes":original.len(),"sha256":format!("{:x}",Sha256::digest(&original))});
    if receipt["original"] != expected_original
        || receipt["angular_fields"]
            != serde_json::json!({
        "path":"uvspec-original.txt","shape":[prepared.wavelengths_nm.len(),prepared.umu.len(),prepared.phi_deg.len()],
        "association":FIELD_ASSOCIATION,"units":"W/(m2*sr*nm)","precision":"Float32","columns_start":4,"ordering":"umu-major phi-minor"})
    {
        return Err(invalid(
            "unchanged complete native atmospheric field identity/shape/association required",
        ));
    }
    let observed = observations(
        spec,
        std::str::from_utf8(&original)
            .map_err(|_| invalid("native atmospheric original UTF-8 text required"))?,
    )?;
    if !same(&receipt["observations"], &serde_json::to_value(&observed)?) {
        return Err(invalid(
            "receipt differs from independently reconstructed native atmospheric angular/flux originals",
        ));
    }
    Ok(crate::qualification::NumericalEvidence {
        reference:if spec.model=="transparent_reference" {"transparent cosine-law and exact zero diffuse originals"}else{"original native hemisphere flux and passive source/boundary energy"}.into(),
        scope:"complete native angular sphere and prescribed wavelength rows; separate convergence and physical validation unqualified".into(),
        error_kind:"maximum_relative_angular_flux_or_transparent_cosine_error".into(),
        error:observed.maximum_angular_flux_error,tolerance:spec.relative_tolerance,
    })
}

pub(crate) fn annotate_fields(
    plan: &crate::contracts::ExecutionPlan,
    artifacts: &mut [crate::contracts::ArtifactManifest],
) -> Result<()> {
    let Some(spec) = &plan.atmosphere else {
        return Ok(());
    };
    let mut fields = artifacts.iter_mut().filter(|a| a.path == ORIGINAL_PATH);
    let field = fields
        .next()
        .ok_or_else(|| invalid("complete native atmospheric original artifact required"))?;
    if fields.next().is_some() || field.format != "txt" || field.bytes == 0 {
        return Err(invalid(
            "unique nonempty native atmospheric original text required",
        ));
    }
    field.time_s = None;
    field.association = Some(FIELD_ASSOCIATION.into());
    field.units = Some(FIELD_UNITS.into());
    field.provenance = format!(
        "authoritative unchanged native Float32 libRadtran {SOURCE_SHA256}, {} profile {}; complete original umu-major phi-minor angular sphere, SI spectral units; wavelength and propagation angles are not physical time",
        spec.profile,
        profile_sha256(&spec.profile)?
    );
    Ok(())
}

pub(crate) fn registered(
    store: &crate::storage::Store,
    id: &str,
    plan: &crate::contracts::ExecutionPlan,
    value: &Value,
) -> Result<crate::qualification::NumericalEvidence> {
    let spec = plan
        .atmosphere
        .as_ref()
        .ok_or_else(|| invalid("approved atmospheric recipe required"))?;
    let record = store
        .artifact_record(id, ORIGINAL_PATH)?
        .ok_or_else(|| invalid("registered native atmospheric original required"))?;
    if record.format != "txt"
        || record.time_s.is_some()
        || record.association.as_deref() != Some(FIELD_ASSOCIATION)
        || record.units.as_deref() != Some(FIELD_UNITS)
        || value["original"]["sha256"] != record.sha256
        || value["original"]["bytes"].as_u64() != Some(record.bytes)
    {
        return Err(invalid(
            "original atmospheric metadata/identity differs from immutable registration",
        ));
    }
    verify(spec, &store.job_dir(id)?.join("stages/atmosphere"), value)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, AtmosphericReferenceSpec, Value) {
        let mut spec: AtmosphericReferenceSpec =
            serde_json::from_str(include_str!("../examples/atmosphere-reference.json")).unwrap();
        spec.model = "transparent_reference".into();
        let prepared = spec.prepare().unwrap();
        let root = tempfile::tempdir().unwrap();
        let raw = prepared
            .wavelengths_nm
            .iter()
            .zip(&prepared.toa_irradiance_w_m2_nm)
            .map(|(wl, toa)| {
                let mut row = vec![
                    format!("{wl:.3}"),
                    format!("{:.6e}", toa * (-prepared.propagation_direction[2])),
                    "0".into(),
                    "0".into(),
                ];
                row.extend(std::iter::repeat_n(
                    "0".into(),
                    prepared.umu.len() * prepared.phi_deg.len(),
                ));
                row.join(" ")
            })
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(root.path().join("uvspec-original.txt"), &raw).unwrap();
        let mut normalized = serde_json::to_value(&prepared).unwrap();
        normalized.as_object_mut().unwrap().remove("request_sha256");
        let checks = canaries();
        let receipt = serde_json::json!({"schema_version":1,"adapter":"libRadtran","version":"2.0.6","source_sha256":SOURCE_SHA256,
            "profile_sha256":profile_sha256(&spec.profile).unwrap(),"backend":"cpu","solver":"disort","precision":"Float32","reduction_precision":"Float64",
            "executed":true,"software_fallback":false,"input":spec,"request_sha256":digest(&spec).unwrap(),"physical_validation":"unqualified",
            "native_executable":"/nix/store/00000000000000000000000000000000-native/bin/uvspec","native_executable_sha256":"a".repeat(64),
            "sandbox":{"policy":SANDBOX_POLICY,"checks":checks},"prepared":normalized,
            "original":{"path":"uvspec-original.txt","bytes":raw.len(),"sha256":format!("{:x}",Sha256::digest(raw.as_bytes()))},
            "angular_fields":{"path":"uvspec-original.txt","shape":[3,64,32],"association":FIELD_ASSOCIATION,"units":"W/(m2*sr*nm)","precision":"Float32","columns_start":4,"ordering":"umu-major phi-minor"},
            "observations":observations(&spec,&raw).unwrap()});
        (root, spec, receipt)
    }
    #[test]
    fn original_receipts_cannot_change_units_source_profile_power_or_execution() {
        let (root, spec, receipt) = fixture();
        // Pure reconstruction accepts the manufactured numerical fixture. It
        // does not establish actual execution; historical qualification also
        // requires registered bytes, immutable binding and durable process exit.
        verify(&spec, root.path(), &receipt).unwrap();
        for (pointer, value) in [
            ("/executed", serde_json::json!(false)),
            ("/profile_sha256", serde_json::json!("a".repeat(64))),
            ("/angular_fields/units", serde_json::json!("W/m2")),
            ("/angular_fields/shape", serde_json::json!([3, 32, 64])),
            (
                "/prepared/propagation_direction",
                serde_json::json!([0., 0., -1.]),
            ),
            ("/observations/tolerance", serde_json::json!(0.1)),
            (
                "/sandbox/policy",
                serde_json::json!(crate::radiation::SANDBOX_POLICY),
            ),
        ] {
            let mut bad = receipt.clone();
            *bad.pointer_mut(pointer).unwrap() = value;
            assert!(verify(&spec, root.path(), &bad).is_err(), "{pointer}");
        }
        std::fs::write(
            root.path().join("uvspec-original.txt"),
            std::fs::read_to_string(root.path().join("uvspec-original.txt"))
                .unwrap()
                .replace("280.000", "281.000"),
        )
        .unwrap();
        assert!(verify(&spec, root.path(), &receipt).is_err());
    }

    #[test]
    fn registered_atmosphere_originals_cannot_be_replaced_or_promote_unexecuted_fixtures() {
        use crate::{
            contracts::ExecutionPlan,
            qualification::{EvidenceState, inspect},
            storage::{Store, ingest_native_tree},
        };
        let (root, spec, receipt) = fixture();
        let state = tempfile::tempdir().unwrap();
        let store = Store::open(&state.path().join("state")).unwrap();
        let plan = ExecutionPlan::atmospheric_reference(spec, "research".into()).unwrap();
        let job = store.submit(&plan, "manufactured-numerical-only").unwrap();
        let destination = store.job_dir(&job.id).unwrap().join("stages/atmosphere");
        std::fs::create_dir_all(&destination).unwrap();
        std::fs::write(
            root.path().join("atmosphere-receipt.json"),
            serde_json::to_vec(&receipt).unwrap(),
        )
        .unwrap();
        let mut artifacts = ingest_native_tree(
            root.path(),
            &destination,
            plan.observation.max_artifact_bytes,
        )
        .unwrap();
        for record in &mut artifacts {
            record.path = format!("stages/atmosphere/{}", record.path);
        }
        annotate_fields(&plan, &mut artifacts).unwrap();
        store.add_artifacts(&job.id, &artifacts).unwrap();
        registered(&store, &job.id, &plan, &receipt).unwrap();
        let evidence = inspect(&store, &job.id).unwrap();
        assert!(matches!(
            evidence.capabilities[0].runtime_execution,
            EvidenceState::NotObserved
        ));
        assert!(matches!(
            evidence.capabilities[0].numerical_verification,
            EvidenceState::NotAssessed
        ));
        let before = std::fs::read(destination.join("uvspec-original.txt")).unwrap();
        let mut replaced = before.clone();
        replaced.extend(b"\n400 0 0 0\n");
        std::fs::write(destination.join("uvspec-original.txt"), &replaced).unwrap();
        let mut self_consistent = receipt.clone();
        self_consistent["original"]["bytes"] = serde_json::json!(replaced.len());
        self_consistent["original"]["sha256"] =
            serde_json::json!(format!("{:x}", Sha256::digest(&replaced)));
        assert!(registered(&store, &job.id, &plan, &self_consistent).is_err());
        std::fs::write(destination.join("uvspec-original.txt"), before).unwrap();
        assert_eq!(
            serde_json::to_value(inspect(&store, &job.id).unwrap()).unwrap(),
            serde_json::to_value(evidence).unwrap()
        );
    }
    fn canaries() -> Value {
        [
            "operation_closure_only",
            "no_gpu_nodes",
            "no_sysfs",
            "no_host_home",
            "no_session_bus",
            "no_worker_socket",
            "network_namespace_isolated",
            "descriptor_readonly",
        ]
        .into_iter()
        .map(|k| (k.into(), Value::Bool(true)))
        .collect::<serde_json::Map<String, Value>>()
        .into()
    }
}
