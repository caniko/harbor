//! Source-independent native packet/receipt reconstruction and immutable registration.
use crate::{
    Result,
    atmosphere_transfer::{
        AtmosphericComponent, propagation, reconstruct_native_packets, reference,
    },
    atmospheric_transport::{AtmosphericTransportSpec, SANDBOX_POLICY, STAGE},
    contracts::{ArtifactManifest, ExecutionPlan, digest, invalid},
    qualification::NumericalEvidence,
    storage::{Store, safe_path},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;

pub const ASSOCIATION: &str = "native_surface_emitter_spectral_packet";
pub const UNITS: &str = "position:m,propagation:1,cosine:1,pdf:1,weight:W/(m2*nm)";

fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => a.as_f64().zip(b.as_f64()).is_some_and(|(a, b)| {
            a.is_finite() && b.is_finite() && (a - b).abs() <= 2e-11 * a.abs().max(b.abs()) + 1e-14
        }),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len() && a.iter().all(|(k, a)| b.get(k).is_some_and(|b| same(a, b)))
        }
        _ => a == b,
    }
}
fn keys(value: &Value, fields: &[&str]) -> bool {
    value
        .as_object()
        .is_some_and(|o| o.len() == fields.len() && fields.iter().all(|k| o.contains_key(*k)))
}
fn number(v: &Value) -> Result<f64> {
    v.as_f64()
        .filter(|v| v.is_finite())
        .ok_or_else(|| invalid("finite native spectral receipt value required"))
}
fn channels(
    source: &crate::atmosphere::PreparedAtmosphericReference,
    spec: &AtmosphericTransportSpec,
    values: &[f64],
) -> Value {
    json!({"incident":crate::radiation::product_integral(&source.wavelengths_nm,values,&vec![1.;values.len()]),"absorbed":crate::radiation::product_integral(&source.wavelengths_nm,values,&spec.request.receiver.absorptivity),"ageing":crate::radiation::product_integral(&source.wavelengths_nm,values,&spec.request.receiver.ageing_action)})
}
fn channel_checks(actual: &Value, expected: &Value, tolerance: f64) -> Result<Value> {
    let mut checks = serde_json::Map::new();
    let mut maximum = 0_f64;
    if !keys(actual, &["incident", "absorbed", "ageing"]) {
        return Err(invalid("exact native optical channel coverage required"));
    }
    for name in ["incident", "absorbed", "ageing"] {
        let (a, b) = (number(&actual[name])?, number(&expected[name])?);
        if a < 0. || b < 0. || (b == 0. && a != 0.) {
            return Err(invalid(
                "nonnegative original optical response including exact zero required",
            ));
        }
        let error = if b == 0. { 0. } else { (a / b - 1.).abs() };
        if error > tolerance {
            return Err(invalid(
                "unchanged source-derived optical reference gate exceeded",
            ));
        }
        maximum = maximum.max(error);
        checks.insert(name.into(),json!({"native_w_m2":a,"reference_w_m2":b,"relative_error":error,"tolerance":tolerance,"passed":true}));
    }
    Ok(
        json!({"channels":checks,"maximum_normalized_error":maximum,"tolerance":tolerance,"passed":true,"physical_validation":"unqualified"}),
    )
}
fn read_record(root: &Path, record: &Value, path: &str, bound: u64) -> Result<Vec<u8>> {
    let data = crate::worker::read_bounded(&safe_path(root, path)?, bound)?;
    if record["path"] != path
        || record["bytes"].as_u64() != Some(data.len() as u64)
        || data.is_empty()
        || record["sha256"] != format!("{:x}", Sha256::digest(&data))
    {
        return Err(invalid(
            "unique bounded original packet filename, bytes and checksum required",
        ));
    }
    Ok(data)
}

pub(crate) fn verify_receipt(
    spec: &AtmosphericTransportSpec,
    original: &str,
    root: &Path,
    receipt: &Value,
) -> Result<NumericalEvidence> {
    spec.validate()?;
    let surface = reference(&spec.atmosphere, &spec.request, original)?;
    let source = spec.atmosphere.prepare()?;
    let receiver = &spec.request.receiver;
    if receipt["schema_version"] != 1
        || receipt["adapter"] != "Mitsuba"
        || receipt["mitsuba_version"] != "3.9.1"
        || receipt["drjit_version"] != "1.5.0"
        || receipt["backend"] != "cpu"
        || receipt["variant"] != "scalar_spectral"
        || receipt["precision"] != "Float32"
        || receipt["reduction_precision"] != "Float64"
        || receipt["executed"] != true
        || receipt["software_fallback"] != false
        || receipt["physical_validation"] != "unqualified"
        || receipt["source_runtime_evidence"]
            != "requires_independent_registered_originals_qualification"
        || receipt["worker_execution"] != "not_qualified_by_standalone_adapter"
        || receipt["convergence"] != "not_assessed"
        || receipt["source_input"] != serde_json::to_value(&spec.atmosphere)?
        || receipt["receiver_input"] != serde_json::to_value(receiver)?
        || receipt["original_atmosphere_sha256"] != spec.source.original.sha256
        || format!("{:x}", Sha256::digest(original.as_bytes())) != spec.source.original.sha256
        || original.len() as u64 != spec.source.original.bytes
        || receipt["request_sha256"] != digest(&spec.native_request()?)?
    {
        return Err(invalid(
            "exact native ABI, approved source/receiver and unchanged original request identities required",
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
        "original_source_readonly",
    ];
    if receipt["sandbox"]["policy"] != SANDBOX_POLICY
        || !keys(&receipt["sandbox"]["checks"], &canaries)
        || canaries
            .iter()
            .any(|k| receipt["sandbox"]["checks"][k] != true)
    {
        return Err(invalid(
            "all operation-specific CPU and read-only original-source sandbox canaries required",
        ));
    }
    let expected = json!({"direct":channels(&source,spec,&surface.direct_w_m2_nm),"diffuse":channels(&source,spec,&surface.diffuse_w_m2_nm),"incident":channels(&source,spec,&surface.incident_w_m2_nm)});
    let transfer_error = number(&receipt["transfer_relative_conservation_error"])?;
    let area = number(&receipt["sensor_area_m2"])?;
    let history = number(&receipt["history_integral_s"])?;
    let observed = crate::atmosphere::observations(&spec.atmosphere, original)?;
    if !same(&receipt["reference"], &expected["incident"])
        || !same(&receipt["component_references"], &expected)
        || transfer_error < 0.
        || transfer_error > spec.request.maximum_relative_conservation_error
        || !same(&receipt["sensor_area_m2"], &json!(surface.sensor_area_m2))
        || !same(
            &receipt["history_integral_s"],
            &json!(surface.integrated_history_scale_s),
        )
        || receipt["angular_shape"]
            != json!([
                source.wavelengths_nm.len(),
                source.umu.len(),
                source.phi_deg.len()
            ])
        || !same(
            &receipt["source_angular_verification"],
            &json!({"maximum_angular_flux_error":observed.maximum_angular_flux_error,"tolerance":spec.atmosphere.relative_tolerance,"passed":true,"physical_validation":"unqualified"}),
        )
        || !keys(&receipt["native_sensor_area_m2"], &["direct", "diffuse"])
        || ["direct", "diffuse"].iter().any(|k| {
            number(&receipt["native_sensor_area_m2"][k]).map_or(true, |v| {
                v <= 0. || (v / surface.sensor_area_m2 - 1.).abs() > 5e-6
            })
        })
    {
        return Err(invalid(
            "original quadrature, separate direct/diffuse references, physical sensor area and prescribed history required",
        ));
    }
    let rows = original
        .lines()
        .map(|line| {
            line.split_whitespace()
                .map(|x| {
                    x.parse::<f64>()
                        .map_err(|_| invalid("native original numeric rows required"))
                })
                .collect::<Result<Vec<_>>>()
        })
        .collect::<Result<Vec<_>>>()?;
    let mut diffuse = vec![];
    let mut diffuse_ids = vec![];
    let mut zero = 0;
    for (i, mu) in source.umu.iter().enumerate() {
        for (j, phi) in source.phi_deg.iter().enumerate() {
            let values = rows
                .iter()
                .map(|row| {
                    row[4 + i * source.phi_deg.len() + j] * source.angular_cell_solid_angle_sr
                })
                .collect::<Vec<_>>();
            let id = format!("angular-{i:03}-{j:03}");
            if values.iter().all(|v| *v == 0.) {
                zero += 1;
            } else {
                diffuse_ids.push(id.clone());
            }
            diffuse.push(json!({"id":id,"umu_index":i,"phi_index":j,"umu":mu,"phi_deg":phi,"propagation_direction":propagation(*mu,*phi),"irradiance_w_m2_nm":values}));
        }
    }
    let direct_values = observed.direct_normal_w_m2_nm;
    let direct_ids = if direct_values.iter().any(|v| *v != 0.) {
        vec!["solar-direct"]
    } else {
        vec![]
    };
    let mapping = json!({"schema_version":1,"direct":{"id":"solar-direct","propagation_direction":source.propagation_direction,"irradiance_w_m2_nm":direct_values},"diffuse":diffuse,"wavelengths_nm":source.wavelengths_nm,"native_emitter_ids":{"direct":direct_ids,"diffuse":diffuse_ids}});
    if !keys(
        &receipt["derived_emitter_sources"],
        &["path", "sha256", "bytes"],
    ) || receipt["zero_original_angular_cells"] != zero
    {
        return Err(invalid(
            "complete original angular emitter mapping required",
        ));
    }
    let bytes = read_record(
        root,
        &receipt["derived_emitter_sources"],
        "emitter-sources.json",
        32 * 1024 * 1024,
    )?;
    if !same(&serde_json::from_slice::<Value>(&bytes)?, &mapping) {
        return Err(invalid(
            "every unchanged original native angular emitter knot and identity required",
        ));
    }
    let observations = receipt["observations"]
        .as_array()
        .filter(|o| o.len() == receiver.seeds.len())
        .ok_or_else(|| invalid("exact approved seed coverage required"))?;
    let mut maximum = 0_f64;
    for (observation, seed) in observations.iter().zip(&receiver.seeds) {
        if !keys(
            observation,
            &[
                "seed",
                "samples",
                "components",
                "native_channels_w_m2",
                "numerical_verification",
                "exposure_j_m2",
                "native_power_w",
                "energy_j",
                "originals",
                "runtime_s",
            ],
        ) || observation["seed"] != *seed
            || observation["samples"] != receiver.samples
            || number(&observation["runtime_s"])? < 0.
            || !keys(&observation["components"], &["direct", "diffuse"])
        {
            return Err(invalid(
                "unchanged ordered seeds, sample budget and direct/diffuse observations required",
            ));
        }
        let records = observation["originals"]
            .as_array()
            .filter(|r| r.len() == 2)
            .ok_or_else(|| invalid("exactly one canonical original per component required"))?;
        let mut combined = json!({"incident":0.,"absorbed":0.,"ageing":0.});
        for (component, kind) in [
            ("direct", AtmosphericComponent::Direct),
            ("diffuse", AtmosphericComponent::Diffuse),
        ] {
            let matches = records
                .iter()
                .filter(|r| r["component"] == component)
                .collect::<Vec<_>>();
            if matches.len() != 1 || !keys(matches[0], &["component", "path", "sha256", "bytes"]) {
                return Err(invalid(
                    "unique unchanged native component originals required",
                ));
            }
            let path = format!("{component}-{seed}.csv");
            let bound =
                u64::from(receiver.samples) * source.wavelengths_nm.len().div_ceil(4) as u64 * 1024
                    + 1024;
            let bytes = read_record(root, matches[0], &path, bound)?;
            let actual = reconstruct_native_packets(
                &spec.atmosphere,
                &spec.request,
                original,
                kind,
                std::str::from_utf8(&bytes)
                    .map_err(|_| invalid("original native UTF-8 packets required"))?,
            )?;
            let actual = json!({"incident":actual.incident_w_m2,"absorbed":actual.absorbed_w_m2,"ageing":actual.ageing_w_m2});
            let component_receipt = &observation["components"][component];
            let checks =
                channel_checks(&actual, &expected[component], receiver.relative_tolerance)?;
            if !keys(
                component_receipt,
                &["native_channels_w_m2", "numerical_verification"],
            ) || !same(&actual, &component_receipt["native_channels_w_m2"])
                || !same(&checks, &component_receipt["numerical_verification"])
            {
                return Err(invalid(
                    "independent original direct/diffuse optical products and numerical gates required",
                ));
            }
            for name in ["incident", "absorbed", "ageing"] {
                combined[name] = json!(number(&combined[name])? + number(&actual[name])?);
            }
        }
        let checks = channel_checks(
            &combined,
            &expected["incident"],
            receiver.relative_tolerance,
        )?;
        if !same(&combined, &observation["native_channels_w_m2"])
            || !same(&checks, &observation["numerical_verification"])
        {
            return Err(invalid(
                "combined totals must derive from original component packets",
            ));
        }
        maximum = maximum.max(number(&checks["maximum_normalized_error"])?);
        for (field, scale) in [
            ("exposure_j_m2", history),
            ("native_power_w", area),
            ("energy_j", history * area),
        ] {
            let derived = json!({"incident":number(&combined["incident"])?*scale,"absorbed":number(&combined["absorbed"])?*scale,"ageing":number(&combined["ageing"])?*scale});
            if !same(&observation[field], &derived) {
                return Err(invalid(
                    "separate unchanged irradiance, area-dependent power, optical dose and energy required",
                ));
            }
        }
    }
    Ok(NumericalEvidence{reference:"every original atmospheric midpoint and independent native spectral packet optical-product quadrature".into(),scope:"registered source-bound planar CPU transport; source execution, convergence and physical validation remain separate".into(),error_kind:"maximum_relative_combined_optical_channel_error".into(),error:maximum,tolerance:receiver.relative_tolerance})
}

pub(crate) fn annotate_fields(
    plan: &ExecutionPlan,
    artifacts: &mut [ArtifactManifest],
) -> Result<()> {
    let Some(spec) = &plan.atmospheric_transport else {
        return Ok(());
    };
    for seed in &spec.request.receiver.seeds {
        for component in ["direct", "diffuse"] {
            let name = format!("stages/{STAGE}/{component}-{seed}.csv");
            let mut matching = artifacts.iter_mut().filter(|a| a.path == name);
            let field = matching.next().ok_or_else(|| {
                invalid("complete atmospheric transport original artifacts required")
            })?;
            if matching.next().is_some() || field.format != "csv" || field.bytes == 0 {
                return Err(invalid("unique original transport CSV required"));
            }
            field.time_s = None;
            field.association = Some(ASSOCIATION.into());
            field.units = Some(UNITS.into());
            field.provenance = format!(
                "original Float32 {component} native spectral packets; registered atmosphere {} SHA-256 {}; seed {seed} is not physical time; compensated Float64 reductions",
                spec.source.job_id, spec.source.original.sha256
            );
        }
    }
    Ok(())
}

pub(crate) fn registered(
    store: &Store,
    id: &str,
    plan: &ExecutionPlan,
    receipt: &Value,
) -> Result<NumericalEvidence> {
    let spec = plan
        .atmospheric_transport
        .as_ref()
        .ok_or_else(|| invalid("approved source-bound atmospheric transport required"))?;
    crate::atmospheric_transport::registered_source(store, spec)?;
    for (path, bound, packet) in std::iter::once((
        "source-atmosphere-original.txt".into(),
        32 * 1024 * 1024,
        false,
    ))
    .chain(std::iter::once((
        "source-atmosphere-receipt.json".into(),
        256 * 1024,
        false,
    )))
    .chain(std::iter::once((
        format!("stages/{STAGE}/emitter-sources.json"),
        32 * 1024 * 1024,
        false,
    )))
    .chain(spec.request.receiver.seeds.iter().flat_map(|seed| {
        ["direct", "diffuse"].map(move |component| {
            (
                format!("stages/{STAGE}/{component}-{seed}.csv"),
                u64::from(spec.request.receiver.samples)
                    * spec.request.receiver.wavelengths.len().div_ceil(4) as u64
                    * 1024
                    + 1024,
                true,
            )
        })
    })) {
        let record = store
            .artifact_record(id, &path)?
            .ok_or_else(|| invalid("immutable original transport registration required"))?;
        let observed = crate::storage::native_manifest(
            &store.job_dir(id)?,
            &path,
            bound,
            "verify original atmospheric transport registration",
        )?;
        if record.sha256 != observed.sha256
            || record.bytes != observed.bytes
            || record.time_s.is_some()
            || (packet
                && (record.format != "csv"
                    || record.association.as_deref() != Some(ASSOCIATION)
                    || record.units.as_deref() != Some(UNITS)))
        {
            return Err(invalid(
                "registered original atmospheric transport bytes or metadata changed",
            ));
        }
    }
    let original = crate::worker::read_bounded(
        &safe_path(&store.job_dir(id)?, "source-atmosphere-original.txt")?,
        32 * 1024 * 1024,
    )?;
    let (receipt_record, source_receipt) =
        crate::results::registered(store, id, "source-atmosphere-receipt.json")?;
    if receipt_record.sha256 != spec.source.receipt.sha256
        || receipt_record.bytes != spec.source.receipt.bytes
    {
        return Err(invalid(
            "retained source atmospheric receipt differs from original approval",
        ));
    }
    crate::atmosphere_fields::verify_bytes(&spec.atmosphere, &original, &source_receipt)?;
    verify_receipt(
        spec,
        std::str::from_utf8(&original).map_err(|_| invalid("source original UTF-8 required"))?,
        &store.job_dir(id)?.join(format!("stages/{STAGE}")),
        receipt,
    )
}
