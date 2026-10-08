//! Independent reconstruction of the original Float32 facet packets and SI power.
use crate::{
    Result,
    cad_spectral::MaterialTriangleRegion,
    cad_transport::{CadSpectralTransportSpec, SANDBOX_POLICY, STAGE},
    cad_triangles::Triangle,
    contracts::{ArtifactManifest, ExecutionPlan, digest, invalid},
    materials::PhysicalInput,
    qualification::NumericalEvidence,
    storage::{Store, safe_path},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;

pub const ASSOCIATION: &str = "native_original_facet_spectral_packet";
pub const UNITS: &str =
    "position:m,normal:1,towards_source:1,cosine:1,pdf:1,weight:W/(m2*nm),reflectance:1";
const HEADER: &str = "region,facet,sample,knot_offset,x_m,y_m,z_m,normal_x,normal_y,normal_z,towards_source_x,towards_source_y,towards_source_z,native_cosine,native_pdf,native_weight_w_m2_nm_0,native_weight_w_m2_nm_1,native_weight_w_m2_nm_2,native_weight_w_m2_nm_3,native_reflectance_0,native_reflectance_1,native_reflectance_2,native_reflectance_3";

pub(crate) fn read_receipt(path: &Path) -> Result<Value> {
    // Full three-seed facet reductions are a registered artifact, not a wire
    // response. Match the existing bounded registered-JSON evidence allowance.
    Ok(serde_json::from_slice(&crate::worker::read_bounded(
        path,
        256 * 1024,
    )?)?)
}

fn number(value: &Value) -> Result<f64> {
    value
        .as_f64()
        .filter(|n| n.is_finite())
        .ok_or_else(|| invalid("finite original CAD optical value required"))
}
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
fn keys(value: &Value, names: &[&str]) -> bool {
    value
        .as_object()
        .is_some_and(|o| o.len() == names.len() && names.iter().all(|k| o.contains_key(*k)))
}
fn sum(values: impl IntoIterator<Item = f64>) -> f64 {
    let (mut total, mut correction) = (0_f64, 0_f64);
    for value in values {
        let next = total + value;
        correction += if total.abs() >= value.abs() {
            (total - next) + value
        } else {
            (value - next) + total
        };
        total = next;
    }
    total + correction
}
fn subtract(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    sum(a.into_iter().zip(b).map(|(a, b)| a * b))
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn point_on_triangle(point: [f64; 3], vertices: [[f64; 3]; 3], tolerance: f64) -> bool {
    let (a, b, p) = (
        subtract(vertices[1], vertices[0]),
        subtract(vertices[2], vertices[0]),
        subtract(point, vertices[0]),
    );
    let (aa, ab, bb, pa, pb) = (dot(a, a), dot(a, b), dot(b, b), dot(p, a), dot(p, b));
    let denominator = aa * bb - ab * ab;
    if denominator <= 0. {
        return false;
    }
    let (u, v) = (
        (bb * pa - ab * pb) / denominator,
        (aa * pb - ab * pa) / denominator,
    );
    u >= -2e-6
        && v >= -2e-6
        && u + v <= 1. + 2e-6
        && (0..3).all(|i| (vertices[0][i] + u * a[i] + v * b[i] - point[i]).abs() <= tolerance)
}
fn ray_box(point: [f64; 3], direction: [f64; 3], bounds: [f64; 6]) -> bool {
    let (mut lower, mut upper) = (f64::NEG_INFINITY, f64::INFINITY);
    for axis in 0..3 {
        if direction[axis] == 0. {
            if !(bounds[2 * axis]..=bounds[2 * axis + 1]).contains(&point[axis]) {
                return false;
            }
        } else {
            let (a, b) = (
                (bounds[2 * axis] - point[axis]) / direction[axis],
                (bounds[2 * axis + 1] - point[axis]) / direction[axis],
            );
            lower = lower.max(a.min(b));
            upper = upper.min(a.max(b));
        }
    }
    upper > lower.max(0.)
}

fn conversions(
    spec: &CadSpectralTransportSpec,
    region: &MaterialTriangleRegion,
    facets: &[Triangle],
    rows: &[Value],
) -> Result<Vec<[[f64; 3]; 3]>> {
    if facets.len() != rows.len() {
        return Err(invalid(
            "complete original/native facet conversion coverage required",
        ));
    }
    let mut converted = Vec::new();
    for (index, (facet, row)) in facets.iter().zip(rows).enumerate() {
        let vertices = facet.vertices_m.map(|p| p.map(|v| f64::from(v as f32)));
        let rounding = facet
            .vertices_m
            .into_iter()
            .flatten()
            .zip(vertices.into_iter().flatten())
            .map(|(a, b)| (a - b).abs())
            .fold(0_f64, f64::max);
        let vector = cross(
            subtract(vertices[1], vertices[0]),
            subtract(vertices[2], vertices[0]),
        );
        let area = dot(vector, vector).sqrt() / 2.;
        let native = number(&row["native_area_m2"])?;
        if !keys(
            row,
            &[
                "region",
                "facet",
                "original_area_m2",
                "native_area_m2",
                "native_area_relative_error",
                "maximum_vertex_rounding_error_m",
                "native_vertices_m",
                "original_sha256",
            ],
        ) || row["region"] != region.assignment.region_name
            || row["facet"].as_u64() != Some(index as u64)
            || !same(&row["original_area_m2"], &json!(facet.area_m2))
            || row["original_sha256"] != region.original_triangles.sha256
            || row["native_vertices_m"] != json!(vertices)
            || number(&row["maximum_vertex_rounding_error_m"])? != rounding
            || rounding > spec.request.maximum_geometry_rounding_error_m
            || native <= 0.
            || area <= 0.
            || (native / area - 1.).abs() > 1e-6
            || (native / facet.area_m2 - 1.).abs() > 1e-6
            || !same(
                &row["native_area_relative_error"],
                &json!((native / facet.area_m2 - 1.).abs()),
            )
        {
            return Err(invalid(
                "unchanged original geometry, bounded Float32 conversion and fixed native area screen required",
            ));
        }
        converted.push(vertices);
    }
    Ok(converted)
}

fn originals(spec: &CadSpectralTransportSpec, root: &Path) -> Result<Vec<Vec<Triangle>>> {
    let mut regions = Vec::new();
    for region in &spec.scene.regions {
        let record = &region.original_triangles;
        let bytes = crate::worker::read_bounded(&safe_path(root, &record.path)?, record.bytes)?;
        if bytes.len() as u64 != record.bytes
            || format!("{:x}", Sha256::digest(&bytes)) != record.sha256
        {
            return Err(invalid(
                "unchanged complete original CAD STL bytes required",
            ));
        }
        let mesh = crate::cad_triangles::verify_binary_box(&bytes, &region.source.geometry)?;
        if !same(
            &serde_json::to_value(&mesh.assessment)?,
            &serde_json::to_value(&region.geometry)?,
        ) {
            return Err(invalid("original CAD facet assessment changed"));
        }
        regions.push(mesh.triangles);
    }
    Ok(regions)
}

fn reduce(
    spec: &CadSpectralTransportSpec,
    region: &MaterialTriangleRegion,
    facet: &Triangle,
    index: usize,
    means: &[f64],
    references: &[f64],
) -> Result<(Value, f64)> {
    let illumination = spec.request.illumination()?;
    let wavelengths = illumination
        .normalized
        .wavelengths_m
        .iter()
        .map(|v| v * 1e9)
        .collect::<Vec<_>>();
    let material = spec
        .request
        .scene
        .materials
        .iter()
        .find(|m| m.name == region.assignment.material_name)
        .ok_or_else(|| invalid("approved whole-facet material required"))?;
    let PhysicalInput::Known { value: optics, .. } = &material.response else {
        return Err(invalid("known original optics required"));
    };
    let mut weights = std::collections::BTreeMap::from([
        ("incident", vec![1.; means.len()]),
        ("absorbed", optics.absorptivity.clone()),
        ("reflected_outgoing", optics.reflectance.clone()),
    ]);
    if let PhysicalInput::Known { value, .. } = &material.ageing_action {
        weights.insert("ageing", value.clone());
    }
    let (mut channels, mut expected, mut errors, mut power, mut dose, mut energy) = (
        serde_json::Map::new(),
        serde_json::Map::new(),
        serde_json::Map::new(),
        serde_json::Map::new(),
        serde_json::Map::new(),
        serde_json::Map::new(),
    );
    let mut maximum = 0_f64;
    for (name, weight) in weights {
        let value = crate::radiation::product_integral(&wavelengths, means, &weight);
        let reference = crate::radiation::product_integral(&wavelengths, references, &weight);
        if !value.is_finite()
            || !reference.is_finite()
            || value < 0.
            || reference < 0.
            || (reference == 0. && value != 0.)
        {
            return Err(invalid(
                "finite nonnegative original optical channels including exact zero required",
            ));
        }
        let error = if reference == 0. {
            0.
        } else {
            (value / reference - 1.).abs()
        };
        if error > spec.request.relative_tolerance {
            return Err(invalid(
                "unchanged original-point optical numerical gate exceeded",
            ));
        }
        maximum = maximum.max(error);
        channels.insert(name.into(), json!(value));
        expected.insert(name.into(), json!(reference));
        errors.insert(name.into(), json!(error));
        power.insert(name.into(), json!(value * facet.area_m2));
        dose.insert(
            name.into(),
            json!(value * illumination.integrated_history_scale_s),
        );
        energy.insert(
            name.into(),
            json!(value * illumination.integrated_history_scale_s * facet.area_m2),
        );
    }
    let result = json!({"region":region.assignment.region_name,"material":region.assignment.material_name,"facet":index,"geometric_boundary":facet.boundary,"original_area_m2":facet.area_m2,"samples":spec.request.samples_per_triangle,"mean_spectral_irradiance_w_m2_nm":means,"channels_w_m2":channels,"reference_channels_w_m2":expected,"relative_errors":errors,"power_w":power,"dose_j_m2":dose,"energy_j":energy,"ageing_status":if matches!(material.ageing_action,PhysicalInput::Known{..}) {"prescribed_action_no_lifetime_calibration"} else {"missing_inputs"}});
    Ok((result, maximum))
}

pub fn verify_numerical(
    spec: &CadSpectralTransportSpec,
    source: &Path,
    root: &Path,
    receipt: &Value,
) -> Result<NumericalEvidence> {
    spec.validate()?;
    let illumination = spec.request.illumination()?;
    if receipt["schema_version"] != 1
        || receipt["adapter"] != "Mitsuba"
        || receipt["versions"] != json!({"mitsuba":"3.9.1","drjit":"1.5.0"})
        || receipt["backend"] != "cpu"
        || receipt["variant"] != "scalar_spectral"
        || receipt["precision"] != "Float32"
        || receipt["reduction_precision"] != "Float64_compensated"
        || receipt["formulation"] != spec.request.formulation
        || receipt["physical_validation"] != "unqualified"
        || receipt["sampling_convergence"] != "not_assessed"
        || receipt["interreflection"] != "excluded_by_explicit_direct_only_model"
        || receipt["input"] != spec.native_request()?
        || !same(
            &receipt["history_integral_s"],
            &json!(illumination.integrated_history_scale_s),
        )
    {
        return Err(invalid(
            "exact original CAD optical inputs, native ABI, history and separate evidence statuses required",
        ));
    }
    let regions = originals(spec, source)?;
    let all = receipt["geometry_conversions"]
        .as_array()
        .filter(|c| c.len() == regions.iter().map(Vec::len).sum::<usize>())
        .ok_or_else(|| invalid("complete native original-facet conversions required"))?;
    let mut vertices = Vec::new();
    let mut cursor = 0;
    for (region, facets) in spec.scene.regions.iter().zip(&regions) {
        vertices.push(conversions(
            spec,
            region,
            facets,
            &all[cursor..cursor + facets.len()],
        )?);
        cursor += facets.len();
    }
    let observations = receipt["observations"]
        .as_array()
        .filter(|o| o.len() == 3)
        .ok_or_else(|| invalid("complete ordered native seeds required"))?;
    let n = illumination.normalized.wavelengths_m.len();
    let crate::radiation::SpectralSource::Directional {
        propagation_direction,
        ..
    } = spec.request.source
    else {
        return Err(invalid("direct optical source required"));
    };
    let mut maximum = 0_f64;
    for (seed, observation) in spec.request.seeds.iter().zip(observations) {
        let path = format!("triangles-{seed}.csv");
        let packet_count =
            cursor as u64 * u64::from(spec.request.samples_per_triangle) * (n as u64).div_ceil(4);
        let bytes = crate::worker::read_bounded(
            &safe_path(root, &path)?,
            packet_count * 1024 + HEADER.len() as u64 + 2,
        )?;
        if !keys(observation, &["seed", "original", "facets"])
            || observation["seed"] != *seed
            || !keys(&observation["original"], &["path", "sha256", "bytes"])
            || observation["original"]["path"] != path
            || observation["original"]["bytes"].as_u64() != Some(bytes.len() as u64)
            || observation["original"]["sha256"] != format!("{:x}", Sha256::digest(&bytes))
        {
            return Err(invalid(
                "original complete optical packet seed, filename, bytes and checksum required",
            ));
        }
        let original =
            std::str::from_utf8(&bytes).map_err(|_| invalid("original optical CSV encoding"))?;
        let mut lines = original.lines();
        if lines.next() != Some(HEADER) {
            return Err(invalid("complete original optical CSV header required"));
        }
        let recorded = observation["facets"]
            .as_array()
            .filter(|f| f.len() == cursor)
            .ok_or_else(|| invalid("complete original facet scientific reductions required"))?;
        let mut result_index = 0;
        for (region_index, (region, facets)) in spec.scene.regions.iter().zip(&regions).enumerate()
        {
            let material = spec
                .request
                .scene
                .materials
                .iter()
                .find(|m| m.name == region.assignment.material_name)
                .ok_or_else(|| invalid("known original region material required"))?;
            let PhysicalInput::Known { value: optics, .. } = &material.response else {
                return Err(invalid("original opaque optics required"));
            };
            for (index, facet) in facets.iter().enumerate() {
                let mut sums = vec![Vec::new(); n];
                let mut references = vec![Vec::new(); n];
                for sample in 0..spec.request.samples_per_triangle {
                    let mut point = None;
                    for offset in (0..n).step_by(4) {
                        let line = lines
                            .next()
                            .ok_or_else(|| invalid("truncated native facet observations"))?;
                        let columns: Vec<_> = line.split(',').collect();
                        if columns.len() != 23
                            || columns[0] != region.assignment.region_name
                            || columns[1] != index.to_string()
                            || columns[2] != sample.to_string()
                            || columns[3] != offset.to_string()
                        {
                            return Err(invalid(
                                "exact original facet/sample/spectral coverage required",
                            ));
                        }
                        let values=columns[4..].iter().map(|s|s.parse::<f64>().ok().filter(|v|v.is_finite() && f64::from(*v as f32)==*v).ok_or_else(||invalid("unchanged exact Float32 original packet values required"))).collect::<Result<Vec<_>>>()?;
                        let position = [values[0], values[1], values[2]];
                        let normal = [values[3], values[4], values[5]];
                        let direction = [values[6], values[7], values[8]];
                        let cosine = values[9];
                        let pdf = values[10];
                        if point.is_some_and(|p| p != position)
                            || !point_on_triangle(
                                position,
                                vertices[region_index][index],
                                spec.request.maximum_geometry_rounding_error_m,
                            )
                            || (0..3).any(|i| {
                                (normal[i] - facet.normal[i]).abs() > 2e-6
                                    || (direction[i] + propagation_direction[i]).abs() > 2e-6
                            })
                        {
                            return Err(invalid(
                                "original facet association, native surface point, direction and outward normal required",
                            ));
                        }
                        point = Some(position);
                        let expected_cosine = (-dot(facet.normal, propagation_direction)).max(0.);
                        let visible = !spec.scene.regions.iter().any(|other| {
                            other.assignment.region_name != region.assignment.region_name
                                && ray_box(position, direction, other.source.geometry.bounds_m)
                        });
                        let expected = if visible && expected_cosine > 0. {
                            1.
                        } else {
                            0.
                        };
                        if (cosine - expected_cosine).abs() > 2e-6
                            || ![0., 1.].contains(&pdf)
                            || (expected_cosine > 0. && pdf != expected)
                        {
                            return Err(invalid(
                                "independent complete original-box visibility and cosine required",
                            ));
                        }
                        for i in 0..4 {
                            let knot = (offset + i).min(n - 1);
                            let source = illumination.normalized.source_values_si[knot] * 1e-9;
                            let weight = values[11 + i];
                            let reflectance = values[15 + i];
                            let expected_weight = if pdf == 1. { source } else { 0. };
                            if (weight - expected_weight).abs() > 2e-6 * expected_weight.abs()
                                || (reflectance - optics.reflectance[knot]).abs()
                                    > (2e-6 * optics.reflectance[knot].abs()).max(1e-8)
                            {
                                return Err(invalid(
                                    "unchanged original native source and optical response required",
                                ));
                            }
                            if offset + i < n {
                                sums[knot].push(weight * cosine);
                                references[knot].push(source * expected_cosine * expected);
                            }
                        }
                    }
                }
                let means = sums
                    .into_iter()
                    .map(|v| sum(v) / f64::from(spec.request.samples_per_triangle))
                    .collect::<Vec<_>>();
                let reference = references
                    .into_iter()
                    .map(|v| sum(v) / f64::from(spec.request.samples_per_triangle))
                    .collect::<Vec<_>>();
                let (result, error) = reduce(spec, region, facet, index, &means, &reference)?;
                if !same(&recorded[result_index], &result) {
                    return Err(invalid(
                        "independently reconstructed original facet power/dose/ageing differs from receipt",
                    ));
                }
                maximum = maximum.max(error);
                result_index += 1;
            }
        }
        if lines.next().is_some() {
            return Err(invalid("extra original optical packets rejected"));
        }
    }
    Ok(NumericalEvidence{reference:"independent original-point complete-box visibility and material spectral products".into(),scope:"original Float32 facets/packets; original-area direct/absorbed/outgoing-reflected power and prescribed dose; no sampling convergence or physical inference".into(),error_kind:"maximum_original_facet_channel_relative_error".into(),error:maximum,tolerance:spec.request.relative_tolerance})
}

pub fn verify_receipt(
    spec: &CadSpectralTransportSpec,
    source: &Path,
    root: &Path,
    receipt: &Value,
) -> Result<NumericalEvidence> {
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
    if receipt["executed"] != true
        || receipt["software_fallback"] != false
        || receipt["request_sha256"] != digest(&spec.native_request()?)?
        || receipt["sandbox"]["policy"] != SANDBOX_POLICY
        || !keys(&receipt["sandbox"]["checks"], &canaries)
        || canaries
            .iter()
            .any(|k| receipt["sandbox"]["checks"][k] != true)
    {
        return Err(invalid(
            "complete independent CAD optical sandbox/execution/request identity required",
        ));
    }
    verify_numerical(spec, source, root, receipt)
}

pub(crate) fn annotate(plan: &ExecutionPlan, artifacts: &mut [ArtifactManifest]) -> Result<()> {
    let Some(spec) = &plan.cad_transport else {
        return Ok(());
    };
    for seed in spec.request.seeds {
        let path = format!("stages/{STAGE}/triangles-{seed}.csv");
        let records = artifacts
            .iter_mut()
            .filter(|a| a.path == path)
            .collect::<Vec<_>>();
        let [record]: [&mut ArtifactManifest; 1] = records
            .try_into()
            .map_err(|_| invalid("unique complete optical originals required"))?;
        record.units = Some(UNITS.into());
        record.association = Some(ASSOCIATION.into());
        record.time_s = None;
        record.provenance="authoritative Float32 original-facet spectral packet; original areas, independent numerical reconstruction and explicit source history retained separately".into();
    }
    Ok(())
}

pub(crate) fn verify_registered(
    store: &Store,
    id: &str,
    plan: &ExecutionPlan,
    receipt: &Value,
) -> Result<NumericalEvidence> {
    let source = crate::cad_transport::registered(store, id, plan)?;
    let spec = plan
        .cad_transport
        .as_ref()
        .ok_or_else(|| invalid("registered approved CAD optical plan required"))?;
    for seed in spec.request.seeds {
        let path = format!("stages/{STAGE}/triangles-{seed}.csv");
        let record = store
            .artifact_record(id, &path)?
            .ok_or_else(|| invalid("registered complete original optical packet required"))?;
        let observed = crate::storage::native_manifest(
            &store.job_dir(id)?,
            &path,
            record.bytes,
            "verify original optical evidence",
        )?;
        if record.format != "csv"
            || record.units.as_deref() != Some(UNITS)
            || record.association.as_deref() != Some(ASSOCIATION)
            || record.time_s.is_some()
            || record.sha256 != observed.sha256
            || record.bytes != observed.bytes
        {
            return Err(invalid(
                "registered complete optical identity/units/association changed",
            ));
        }
    }
    verify_receipt(
        spec,
        &source,
        &safe_path(&store.job_dir(id)?, &format!("stages/{STAGE}"))?,
        receipt,
    )
}

#[cfg(test)]
mod receipt_capacity_tests {
    use super::*;
    #[test]
    fn original_facet_receipt_capacity_is_distinct_from_protocol_message_capacity() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("receipt.json");
        let value = json!({"original_packet_shape_description": "x".repeat(128*1024)});
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(read_receipt(&path).unwrap(), value);
        std::fs::write(&path, " ".repeat(256 * 1024 + 1)).unwrap();
        assert!(read_receipt(&path).is_err());
        let link = root.path().join("alias.json");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(read_receipt(&link).is_err());
    }
}
