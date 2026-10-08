//! Independent reconstruction of authoritative native directional spectral CSV.
use crate::{
    Result,
    contracts::{digest, invalid},
    radiation::{SpectralReferenceSpec, SpectralSource, product_integral},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::Path;

fn number(value: &Value) -> Result<f64> {
    value
        .as_f64()
        .filter(|v| v.is_finite())
        .ok_or_else(|| invalid("finite original spectral value required"))
}
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-11 * a.abs().max(b.abs()) + 1e-15
}
fn same_scale(a: f64, b: f64) -> bool {
    (a - b).abs() <= 2e-12 * a.abs().max(b.abs())
}
fn scalar(text: &str) -> Result<f64> {
    text.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| invalid("finite original spectral scalar required"))
}
fn keys(value: &Value, expected: &[&str]) -> bool {
    value
        .as_object()
        .is_some_and(|v| v.len() == expected.len() && expected.iter().all(|k| v.contains_key(*k)))
}
fn numeric_array(value: &Value, expected: &[f64]) -> Result<()> {
    let values = value
        .as_array()
        .filter(|v| v.len() == expected.len())
        .ok_or_else(|| invalid("complete spectral normalization array required"))?;
    for (actual, expected) in values.iter().zip(expected) {
        if !same_scale(number(actual)?, *expected) {
            return Err(invalid("spectral SI normalization changed"));
        }
    }
    Ok(())
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}

pub(crate) fn original(
    spec: &SpectralReferenceSpec,
    root: &Path,
    observation: &Value,
) -> Result<Vec<u8>> {
    let seed = observation["seed"]
        .as_u64()
        .filter(|v| spec.seeds.iter().any(|s| u64::from(*s) == *v))
        .ok_or_else(|| invalid("approved native spectral seed required"))?;
    let path = format!("directional-{seed}.csv");
    if observation["path"] != path || observation["samples"] != spec.samples {
        return Err(invalid(
            "exact native spectral path and sample budget required",
        ));
    }
    let limit = u64::from(spec.samples) * (256 + 32 * spec.wavelengths.len() as u64);
    let bytes = crate::worker::read_bounded(&crate::storage::safe_path(root, &path)?, limit)?;
    if observation["bytes"].as_u64() != Some(bytes.len() as u64)
        || observation["sha256"] != format!("{:x}", Sha256::digest(&bytes))
    {
        return Err(invalid(
            "unchanged complete original directional spectral bytes required",
        ));
    }
    Ok(bytes)
}

fn channels(spec: &SpectralReferenceSpec, bytes: &[u8]) -> Result<[f64; 3]> {
    let prepared = spec.prepare()?;
    let SpectralSource::Directional {
        propagation_direction,
        ..
    } = &spec.source
    else {
        return Err(invalid("directional original CSV required"));
    };
    let native_cosine = (-dot(*propagation_direction, spec.sensor_normal)).max(0.);
    let up = if spec.sensor_normal[1].abs() < 0.9 {
        [0., 1., 0.]
    } else {
        [1., 0., 0.]
    };
    let mut right = cross(up, spec.sensor_normal);
    let norm = dot(right, right).sqrt();
    right.iter_mut().for_each(|v| *v /= norm);
    let vertical = cross(spec.sensor_normal, right);
    let n = spec.wavelengths.len();
    let header = format!(
        "sample,x_m,y_m,z_m,native_cosine,{}",
        (0..n)
            .map(|i| format!("emitter_weight_w_m2_nm_{i}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    let text =
        std::str::from_utf8(bytes).map_err(|_| invalid("UTF-8 directional originals required"))?;
    let mut rows = text.lines();
    if rows.next() != Some(header.as_str()) {
        return Err(invalid(
            "exact ordered native sample/position/cosine/spectral-knot columns required",
        ));
    }
    let mut total = vec![0.; n];
    let mut correction = vec![0.; n];
    let mut count = 0;
    for row in rows {
        let row = row.split(',').collect::<Vec<_>>();
        if count >= spec.samples || row.len() != 5 + n || row[0].parse::<u32>().ok() != Some(count)
        {
            return Err(invalid(
                "complete ordered native directional sample identities required",
            ));
        }
        let p = [scalar(row[1])?, scalar(row[2])?, scalar(row[3])?];
        let [width, height] = prepared.normalized.sensor_size_m;
        let geometric_roundoff = 5e-6 * width.max(height);
        if dot(p, spec.sensor_normal).abs() > geometric_roundoff
            || dot(p, right).abs() > width / 2. + geometric_roundoff
            || dot(p, vertical).abs() > height / 2. + geometric_roundoff
        {
            return Err(invalid(
                "every native sample must retain the approved physical sensor plane and rectangle",
            ));
        }
        let cosine = scalar(row[4])?;
        if !(0. ..=1.000005).contains(&cosine) || (cosine - native_cosine).abs() > 5e-6 {
            return Err(invalid(
                "native directional cosine differs from approved orientation",
            ));
        }
        for i in 0..n {
            let weight = scalar(row[5 + i])?;
            let source = prepared.normalized.source_values_si[i] * 1e-9;
            if weight < 0.
                || (spec.occlusion == "full_directional_occluder" && weight != 0.)
                || (spec.occlusion == "none"
                    && native_cosine > 0.
                    && (weight - source).abs() > 5e-6 * source.abs())
                || weight > source * (1. + 5e-6)
            {
                return Err(invalid(
                    "native UV emitter knot/visibility changed or exceeds explicit Float32 roundoff",
                ));
            }
            // Compensated Float64 sum; retain the original Float32 observations.
            let y = weight * cosine - correction[i];
            let next = total[i] + y;
            correction[i] = (next - total[i]) - y;
            total[i] = next;
        }
        count += 1;
    }
    if count != spec.samples {
        return Err(invalid("native directional sample originals truncated"));
    }
    let means = total
        .into_iter()
        .map(|v| v / f64::from(count) * 1e9)
        .collect::<Vec<_>>();
    let x = &prepared.normalized.wavelengths_m;
    Ok([
        product_integral(x, &means, &vec![1.; n]),
        product_integral(x, &means, &spec.absorptivity),
        product_integral(x, &means, &spec.ageing_action),
    ])
}

pub(crate) fn verify(
    spec: &SpectralReferenceSpec,
    root: &Path,
    value: &Value,
) -> Result<crate::qualification::NumericalEvidence> {
    let prepared = spec.prepare()?;
    if !matches!(spec.source, SpectralSource::Directional { .. })
        || value["schema_version"] != 1
        || value["adapter"] != "Mitsuba"
        || value["mitsuba_version"] != "3.9.1"
        || value["drjit_version"] != "1.5.0"
        || value["variant"] != "scalar_spectral"
        || value["backend"] != "cpu"
        || value["precision"] != "Float32"
        || value["reduction_precision"] != "Float64"
        || value["executed"] != true
        || value["software_fallback"] != false
        || value["physical_validation"] != "unqualified"
        || value["input"] != serde_json::to_value(spec)?
        || value["request_sha256"] != digest(spec)?
        || !value["reflection_model_assessment"].is_null()
    {
        return Err(invalid(
            "exact source-bound directional spectral ABI, request, precision and executed identity required",
        ));
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
    if value["sandbox"]["policy"] != crate::radiation::SANDBOX_POLICY
        || !keys(&value["sandbox"]["checks"], &canaries)
        || canaries
            .into_iter()
            .any(|name| value["sandbox"]["checks"][name] != true)
    {
        return Err(invalid(
            "complete operation-specific spectral isolation evidence required",
        ));
    }
    let normalized = &value["normalized"];
    if !keys(
        normalized,
        &[
            "wavelengths_nm",
            "source_values_nm",
            "source_unit",
            "sensor_size_m",
            "sensor_area_m2",
            "weights",
            "reference",
            "base_integrals",
            "angular_factor",
            "history_times_s",
            "history_scales",
            "history_integral_s",
            "relative_tolerance",
        ],
    ) || normalized["source_unit"] != "W/(m2*nm)"
        || number(&normalized["relative_tolerance"])? != spec.relative_tolerance
    {
        return Err(invalid(
            "strict spectral normalization and unchanged analytical gate required",
        ));
    }
    numeric_array(
        &normalized["wavelengths_nm"],
        &prepared
            .normalized
            .wavelengths_m
            .iter()
            .map(|v| v * 1e9)
            .collect::<Vec<_>>(),
    )?;
    numeric_array(
        &normalized["source_values_nm"],
        &prepared
            .normalized
            .source_values_si
            .iter()
            .map(|v| v * 1e-9)
            .collect::<Vec<_>>(),
    )?;
    for (name, expected) in [
        ("sensor_area_m2", prepared.normalized.sensor_area_m2),
        (
            "angular_factor",
            prepared.normalized.angular_reference_factor,
        ),
        ("history_integral_s", prepared.integrated_history_scale_s),
    ] {
        if !same_scale(number(&normalized[name])?, expected) {
            return Err(invalid(
                "approved spectral SI area/orientation/dose model changed",
            ));
        }
    }
    for (name, expected) in [
        (
            "sensor_size_m",
            prepared.normalized.sensor_size_m.as_slice(),
        ),
        (
            "history_times_s",
            prepared.normalized.history_times_s.as_slice(),
        ),
        (
            "history_scales",
            prepared.normalized.history_scales.as_slice(),
        ),
    ] {
        numeric_array(&normalized[name], expected)?;
    }
    let expected = [
        prepared.incident_irradiance_w_m2,
        prepared.absorbed_irradiance_w_m2,
        prepared.ageing_weighted_irradiance_w_m2,
    ];
    let weights = [
        vec![1.; spec.wavelengths.len()],
        spec.absorptivity.clone(),
        spec.ageing_action.clone(),
    ];
    let names = ["incident", "absorbed", "ageing"];
    for group in ["weights", "reference", "base_integrals"] {
        if !keys(&normalized[group], &names) {
            return Err(invalid(
                "three distinct complete UV optical channels required",
            ));
        }
    }
    for ((name, weights), reference) in names.into_iter().zip(weights).zip(expected) {
        numeric_array(&normalized["weights"][name], &weights)?;
        let base = product_integral(
            &prepared.normalized.wavelengths_m,
            &prepared.normalized.source_values_si,
            &weights,
        );
        if !close(number(&normalized["base_integrals"][name])?, base)
            || !close(number(&normalized["reference"][name])?, reference)
        {
            return Err(invalid(
                "independent exact UV optical-product reference changed",
            ));
        }
    }
    let observations = value["observations"]
        .as_array()
        .filter(|v| v.len() == spec.seeds.len())
        .ok_or_else(|| invalid("complete ordered native spectral seeds required"))?;
    let mut maximum = 0f64;
    for (observation, seed) in observations.iter().zip(spec.seeds) {
        if observation["seed"] != seed
            || observation["method"]
                != "native_emitter_direction_visibility_and_surface_position; exact_original_knot_product_quadrature"
            || number(&observation["runtime_s"])? < 0.
            || !keys(&observation["native_channels_w_m2"], &names)
            || !keys(&observation["exposure_j_m2"], &names)
        {
            return Err(invalid(
                "exact original directional observation semantics required",
            ));
        }
        let actual = channels(spec, &original(spec, root, observation)?)?;
        let checks = &observation["numerical_verification"];
        if checks["passed"] != true
            || checks["physical_validation"] != "unqualified"
            || number(&checks["tolerance"])? != spec.relative_tolerance
            || !keys(&checks["channels"], &names)
        {
            return Err(invalid(
                "unchanged independent spectral analytical checks required",
            ));
        }
        let mut local = 0f64;
        for ((name, actual), reference) in names.into_iter().zip(actual).zip(expected) {
            let error = if reference == 0. {
                if actual != 0. {
                    return Err(invalid(
                        "zero optical/back-facing/occluded originals must remain zero",
                    ));
                }
                0.
            } else {
                (actual / reference - 1.).abs()
            };
            let check = &checks["channels"][name];
            if error > spec.relative_tolerance
                || actual < 0.
                || !close(number(&observation["native_channels_w_m2"][name])?, actual)
                || !close(
                    number(&observation["exposure_j_m2"][name])?,
                    actual * prepared.integrated_history_scale_s,
                )
                || !close(number(&check["native_w_m2"])?, actual)
                || !close(number(&check["reference_w_m2"])?, reference)
                || !close(number(&check["relative_error"])?, error)
                || number(&check["tolerance"])? != spec.relative_tolerance
                || check["passed"] != true
            {
                return Err(invalid(
                    "original native UV optical integrals/dose differ from unchanged independent gate",
                ));
            }
            local = local.max(error);
        }
        if !close(number(&checks["maximum_normalized_error"])?, local) {
            return Err(invalid("complete native spectral maximum error required"));
        }
        maximum = maximum.max(local);
    }
    Ok(crate::qualification::NumericalEvidence {reference:"independent complete original directional knots, explicit UV products and prescribed piecewise-linear dose".into(),
        scope:"synthetic directional planar CPU reference; every native sample/seed, SI sensor area/orientation and visibility; no atmospheric, temperature, ageing-lifetime or physical qualification".into(),
        error_kind:"maximum_relative_irradiance_error".into(),error:maximum,tolerance:spec.relative_tolerance})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{contracts::digest, radiation::SpectralReferenceSpec};
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};
    use std::fs;

    fn fixture() -> (tempfile::TempDir, SpectralReferenceSpec, Value) {
        let root = tempfile::tempdir().unwrap();
        let mut spec: SpectralReferenceSpec =
            serde_json::from_slice(include_bytes!("../examples/spectral-reference.json")).unwrap();
        spec.samples = 1024;
        let reference = spec.prepare().unwrap();
        let mut observations = vec![];
        for seed in spec.seeds {
            let mut csv="sample,x_m,y_m,z_m,native_cosine,emitter_weight_w_m2_nm_0,emitter_weight_w_m2_nm_1\n".to_string();
            for i in 0..spec.samples {
                csv.push_str(&format!("{i},0,0,0,1,1,3\n"));
            }
            let path = format!("directional-{seed}.csv");
            fs::write(root.path().join(&path), csv.as_bytes()).unwrap();
            let channels = json!({"incident":240.,"absorbed":132.,"ageing":170.});
            let mut checks = serde_json::Map::new();
            for (channel, value) in channels.as_object().unwrap() {
                checks.insert(channel.clone(),json!({"native_w_m2":value,"reference_w_m2":value,"relative_error":0.,"tolerance":spec.relative_tolerance,"passed":true}));
            }
            observations.push(json!({"seed":seed,"samples":spec.samples,
                "method":"native_emitter_direction_visibility_and_surface_position; exact_original_knot_product_quadrature",
                "native_channels_w_m2":channels,"numerical_verification":{"channels":checks,"maximum_normalized_error":0.,"tolerance":spec.relative_tolerance,"passed":true,"physical_validation":"unqualified"},
                "path":path,"sha256":format!("{:x}",Sha256::digest(csv.as_bytes())),"bytes":csv.len(),"runtime_s":0.1,
                "exposure_j_m2":{"incident":864000.,"absorbed":475200.,"ageing":612000.}}));
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
        let checks: serde_json::Map<String, Value> = canaries
            .into_iter()
            .map(|k| (k.into(), json!(true)))
            .collect();
        let value = json!({"schema_version":1,"adapter":"Mitsuba","mitsuba_version":"3.9.1","drjit_version":"1.5.0","variant":"scalar_spectral","backend":"cpu","precision":"Float32","reduction_precision":"Float64","executed":true,"software_fallback":false,
            "input":spec,"request_sha256":digest(&spec).unwrap(),"physical_validation":"unqualified","reflection_model_assessment":null,
            "sandbox":{"policy":crate::radiation::SANDBOX_POLICY,"checks":checks},
            "normalized":{"wavelengths_nm":[280.,400.],"source_values_nm":[1.,3.],"source_unit":"W/(m2*nm)","sensor_size_m":reference.normalized.sensor_size_m,"sensor_area_m2":reference.normalized.sensor_area_m2,
                "weights":{"incident":[1.,1.],"absorbed":spec.absorptivity,"ageing":spec.ageing_action},"reference":{"incident":240.,"absorbed":132.,"ageing":170.},
                "base_integrals":{"incident":240.,"absorbed":132.,"ageing":170.},"angular_factor":1.,"history_times_s":reference.normalized.history_times_s,"history_scales":reference.normalized.history_scales,"history_integral_s":3600.,"relative_tolerance":spec.relative_tolerance},
            "observations":observations});
        (root, spec, value)
    }

    #[test]
    fn originals_bind_every_seed_position_knot_optical_integral_and_prescribed_dose() {
        let (root, spec, value) = fixture();
        let evidence = verify(&spec, root.path(), &value).unwrap();
        assert!(evidence.error < 1e-12);
        for (field, replacement) in [
            ("executed", json!(false)),
            ("mitsuba_version", json!("3.8.0")),
            ("request_sha256", json!("f".repeat(64))),
        ] {
            let mut changed = value.clone();
            changed[field] = replacement;
            assert!(verify(&spec, root.path(), &changed).is_err());
        }
        let mut changed = value.clone();
        changed["observations"][0]["exposure_j_m2"]["absorbed"] = json!(156. * 3600.);
        assert!(verify(&spec, root.path(), &changed).is_err());
        let mut changed = value.clone();
        changed["observations"][0]["numerical_verification"]["tolerance"] = json!(0.1);
        assert!(verify(&spec, root.path(), &changed).is_err());
        let mut changed = value.clone();
        changed["observations"].as_array_mut().unwrap().pop();
        assert!(verify(&spec, root.path(), &changed).is_err());
        // Rehashing altered original positions/knots cannot make them approved science.
        let original = fs::read_to_string(root.path().join("directional-1.csv")).unwrap();
        for replacement in [
            "0,1,0,0,1,1,3\n",
            "0,0,0,0,0.8,1,3\n",
            "0,0,0,0,1,1,4\n",
            "0,0,0,0,1,NaN,3\n",
        ] {
            let changed_bytes = original.replacen("0,0,0,0,1,1,3\n", replacement, 1);
            fs::write(
                root.path().join("directional-1.csv"),
                changed_bytes.as_bytes(),
            )
            .unwrap();
            let mut changed = value.clone();
            changed["observations"][0]["sha256"] =
                json!(format!("{:x}", Sha256::digest(changed_bytes.as_bytes())));
            changed["observations"][0]["bytes"] = json!(changed_bytes.len());
            assert!(verify(&spec, root.path(), &changed).is_err());
        }
    }

    #[test]
    fn registered_spectral_originals_cannot_be_replaced_or_promote_manufactured_receipts() {
        use crate::{
            contracts::ExecutionPlan,
            storage::{Store, ingest_native_tree},
        };
        let (originals, spec, value) = fixture();
        let state = tempfile::tempdir().unwrap();
        let store = Store::open(&state.path().join("state")).unwrap();
        let plan = ExecutionPlan::spectral_reference(spec, "research".into()).unwrap();
        let job = store.submit(&plan, "manufactured-numerical-only").unwrap();
        let destination = store.job_dir(&job.id).unwrap().join("stages/spectral");
        fs::create_dir_all(&destination).unwrap();
        fs::write(
            originals.path().join("spectral-receipt.json"),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
        let mut records = ingest_native_tree(
            originals.path(),
            &destination,
            plan.observation.max_artifact_bytes,
        )
        .unwrap();
        for record in &mut records {
            record.path = format!("stages/spectral/{}", record.path);
        }
        crate::radiation::annotate_fields(&plan, &mut records).unwrap();
        store.add_artifacts(&job.id, &records).unwrap();
        crate::radiation::verify_registered(&store, &job.id, &plan, &value).unwrap();
        let historical = crate::qualification::inspect(&store, &job.id).unwrap();
        assert!(matches!(
            historical.capabilities[0].runtime_execution,
            crate::qualification::EvidenceState::NotObserved
        ));
        assert!(matches!(
            historical.capabilities[0].numerical_verification,
            crate::qualification::EvidenceState::NotAssessed
        ));
        let before = fs::read(destination.join("directional-1.csv")).unwrap();
        let mut changed = before.clone();
        changed.extend(b"0,0,0,0,1,1,3\n");
        fs::write(destination.join("directional-1.csv"), &changed).unwrap();
        let mut forged = value.clone();
        forged["observations"][0]["sha256"] = json!(format!("{:x}", Sha256::digest(&changed)));
        forged["observations"][0]["bytes"] = json!(changed.len());
        assert!(crate::radiation::verify_registered(&store, &job.id, &plan, &forged).is_err());
        fs::write(destination.join("directional-1.csv"), before).unwrap();
        assert_eq!(
            serde_json::to_value(crate::qualification::inspect(&store, &job.id).unwrap()).unwrap(),
            serde_json::to_value(historical).unwrap()
        );
    }
}
