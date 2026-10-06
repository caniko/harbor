//! Source-bound constant-capacitance projection of complete native thermal boxes.

use crate::{
    Result,
    contracts::{digest, invalid},
    results::SampleLocation,
    science::Quantity,
    storage::Store,
    thermal_results::{self, ThermalField, ThermalSampleReport, ThermalSampleRequest},
    transfers::*,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LumpedBox {
    pub region: String,
    pub size_m: [f64; 3],
    pub origin_m: [f64; 3],
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThermalProjectionRequest {
    pub schema_version: u32,
    pub source_job: String,
    pub physical_time_s: f64,
    pub destination: LumpedBox,
    pub maximum_projection_error_k: f64,
    pub maximum_relative_conservation_error: f64,
}
impl ThermalProjectionRequest {
    pub fn validate(&self) -> Result<()> {
        ThermalSampleRequest {
            schema_version: self.schema_version,
            job_id: self.source_job.clone(),
            field: ThermalField::Temperature,
            physical_time_s: self.physical_time_s,
            locations: vec![SampleLocation::Node { node_id: 1 }],
        }
        .validate()?;
        if !crate::contracts::token(&self.destination.region)
            || self
                .destination
                .size_m
                .iter()
                .any(|v| !v.is_finite() || *v <= 0.)
            || self.destination.origin_m.iter().any(|v| !v.is_finite())
            || !self.maximum_projection_error_k.is_finite()
            || !(0. ..=1.).contains(&self.maximum_projection_error_k)
            || !self.maximum_relative_conservation_error.is_finite()
            || !(0. ..=1e-10).contains(&self.maximum_relative_conservation_error)
            || self.maximum_relative_conservation_error == 0.
        {
            return Err(invalid(
                "named congruent translated box, explicit bounded Kelvin projection loss and unchanged conservative gate required",
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThermalProjectionReport {
    pub schema_version: u32,
    pub projection_id: String,
    pub request: ThermalProjectionRequest,
    pub source: ThermalSampleReport,
    pub source_nodes: usize,
    pub source_region: String,
    pub source_density_kg_m3: f64,
    pub source_specific_heat_j_kg_k: f64,
    pub material_provenance: String,
    pub destination_mesh_sha256: String,
    pub destination_temperature_k: f64,
    pub capacitance_j_k: f64,
    pub maximum_abs_projection_error_k: f64,
    pub source_temperature_range_k: [f64; 2],
    pub receipt: TransferReceipt,
    pub physical_validation: String,
    pub model: String,
}

pub fn project(
    store: &Store,
    request: &ThermalProjectionRequest,
) -> Result<ThermalProjectionReport> {
    request.validate()?;
    let selection = ThermalSampleRequest {
        schema_version: 1,
        job_id: request.source_job.clone(),
        field: ThermalField::Temperature,
        physical_time_s: request.physical_time_s,
        locations: vec![SampleLocation::Node { node_id: 1 }],
    };
    let verified = thermal_results::verified_sample(store, &selection)?;
    let plan = store.plan(&request.source_job)?;
    let spec = plan
        .thermal
        .as_ref()
        .ok_or_else(|| invalid("source native transient thermal recipe required"))?;
    if request
        .destination
        .size_m
        .iter()
        .zip(spec.size_m)
        .any(|(a, b)| *a != b)
        || request
            .destination
            .origin_m
            .iter()
            .zip(spec.size_m)
            .any(|(origin, size)| {
                !(*origin + size).is_finite()
                    || 16. * (origin.abs().next_up() - origin.abs()) > spec.geometry_tolerance_m
            })
    {
        return Err(invalid(
            "exact congruent destination dimensions and resolved translation required; no scaling or rotation",
        ));
    }
    let fields = verified.fields["times"]
        .as_array()
        .and_then(|v| {
            v.iter()
                .find(|v| v["requested_s"].as_f64() == Some(request.physical_time_s))
        })
        .ok_or_else(|| invalid("exact approved retained temperature state required"))?;
    let projection = project_box(
        &verified.mesh,
        &fields["temperature_k"],
        spec.size_m,
        spec.resolution,
        spec.geometry_tolerance_m,
        [spec.density_kg_m3, spec.specific_heat_j_kg_k],
        request.maximum_projection_error_k,
    )?;
    let mesh_id = digest(
        &serde_json::json!({"schema_version":1,"formulation":"congruent_constant_capacitance_box_projection","coordinate_unit":"m","region":request.destination.region,"size_m":request.destination.size_m,"origin_m":request.destination.origin_m,"association":"cell","cells":1}),
    )?;
    let q = |value| Quantity {
        value,
        unit: "J/K".into(),
    };
    let map = ConservativeTransfer {
        schema_version: 1,
        source_artifact_sha256: verified.report.sample.field_artifact.sha256.clone(),
        quantity: TransferQuantity::Temperature,
        source: TransferEndpoint {
            mesh_sha256: verified.report.sample.mesh_artifact.sha256.clone(),
            region: "entire_box".into(),
            association: Association::Point,
            orientation: [0., 0., 1.],
            measures: projection.capacitances.iter().copied().map(q).collect(),
        },
        destination: TransferEndpoint {
            mesh_sha256: mesh_id.clone(),
            region: request.destination.region.clone(),
            association: Association::Cell,
            orientation: [0., 0., 1.],
            measures: vec![q(projection.capacitance_j_k)],
        },
        normal_mapping: NormalMapping::SameDirection,
        interpolation: "piecewise_constant_overlap".into(),
        overlaps: projection
            .capacitances
            .iter()
            .enumerate()
            .map(|(source, &weight)| Overlap {
                source,
                destination: 0,
                measure: q(weight),
            })
            .collect(),
        maximum_relative_conservation_error: request.maximum_relative_conservation_error,
    };
    let transferred = map.apply(&projection.temperatures, "K")?;
    if transferred.values_si.len() != 1
        || (transferred.values_si[0] - projection.mean_k).abs() > 1e-10
        || (transferred.receipt.source_integral - projection.source_integral_j).abs()
            / projection.source_integral_j
            > 1e-12
    {
        return Err(invalid(
            "independent native capacitance projection disagrees with conservative map",
        ));
    }
    let range = [
        projection
            .temperatures
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min),
        projection
            .temperatures
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max),
    ];
    let mut report=ThermalProjectionReport {schema_version:1,projection_id:String::new(),request:request.clone(),source:verified.report,source_nodes:projection.temperatures.len(),source_region:"entire_box".into(),source_density_kg_m3:spec.density_kg_m3,source_specific_heat_j_kg_k:spec.specific_heat_j_kg_k,material_provenance:spec.material_provenance.clone(),destination_mesh_sha256:mesh_id,destination_temperature_k:transferred.values_si[0],capacitance_j_k:projection.capacitance_j_k,maximum_abs_projection_error_k:projection.maximum_error_k,source_temperature_range_k:range,receipt:transferred.receipt,physical_validation:"unqualified".into(),model:"complete native C3D8 nodal lumped-capacitance projection to explicitly translated congruent uniform box; retained source point temperatures remain authoritative; no heat-flow or contact solve".into()};
    report.projection_id = digest(&report)?;
    Ok(report)
}

struct Projection {
    temperatures: Vec<f64>,
    capacitances: Vec<f64>,
    mean_k: f64,
    capacitance_j_k: f64,
    source_integral_j: f64,
    maximum_error_k: f64,
}

fn project_box(
    mesh: &serde_json::Value,
    field: &serde_json::Value,
    size: [f64; 3],
    n: u32,
    tolerance: f64,
    material: [f64; 2],
    maximum_error_k: f64,
) -> Result<Projection> {
    let positive = |v: f64| v.is_finite() && v > 0.;
    if !(2..=32).contains(&n)
        || size.iter().chain(&material).any(|v| !positive(*v))
        || !positive(tolerance)
        || tolerance >= size.iter().copied().fold(f64::INFINITY, f64::min) / f64::from(n) / 4.
        || !maximum_error_k.is_finite()
        || !(0. ..=1.).contains(&maximum_error_k)
        || mesh["schema_version"] != 1
        || mesh["coordinate_unit"] != "m"
        || mesh["element_type"] != "C3D8"
    {
        return Err(invalid(
            "bounded complete native SI C3D8 box and explicit projection policy required",
        ));
    }
    let nodes: BTreeMap<u64, [f64; 3]> = serde_json::from_value(mesh["nodes"].clone())?;
    let cells: BTreeMap<u64, [u64; 8]> = serde_json::from_value(mesh["elements"].clone())?;
    let values: BTreeMap<u64, f64> = serde_json::from_value(field.clone())?;
    if nodes.len() != (n + 1).pow(3) as usize
        || cells.len() != n.pow(3) as usize
        || nodes.keys().copied().collect::<BTreeSet<_>>() != values.keys().copied().collect()
        || values.values().any(|v| !positive(*v))
    {
        return Err(invalid(
            "complete unique native temperature/element/node coverage required",
        ));
    }
    let mut indices = BTreeMap::new();
    let mut positions = BTreeSet::new();
    for (&id, coordinate) in &nodes {
        if id == 0 {
            return Err(invalid("positive native box node identity required"));
        }
        let mut index = [0u32; 3];
        for axis in 0..3 {
            let tick = (coordinate[axis] / size[axis] * f64::from(n)).round();
            if !coordinate[axis].is_finite()
                || !(0. ..=f64::from(n)).contains(&tick)
                || (coordinate[axis] - tick * size[axis] / f64::from(n)).abs() > tolerance
            {
                return Err(invalid(
                    "native temperature coordinates differ from approved box",
                ));
            }
            index[axis] = tick as u32;
        }
        if !positions.insert(index) {
            return Err(invalid("duplicated native thermal geometry"));
        }
        indices.insert(id, index);
    }
    let volume = size.iter().product::<f64>();
    let capacitance = volume * material[0] * material[1];
    let cell_capacity = capacitance / f64::from(n).powi(3);
    if !positive(volume) || !positive(cell_capacity) || !positive(capacitance) {
        return Err(invalid(
            "finite positive native thermal capacitance required",
        ));
    }
    let mut measures: BTreeMap<_, f64> = nodes.keys().map(|id| (*id, 0.)).collect();
    let mut covered = BTreeSet::new();
    let local = [
        [0, 0, 0],
        [1, 0, 0],
        [1, 1, 0],
        [0, 1, 0],
        [0, 0, 1],
        [1, 0, 1],
        [1, 1, 1],
        [0, 1, 1],
    ];
    for (&tag, ids) in &cells {
        if tag == 0 || ids.iter().copied().collect::<BTreeSet<_>>().len() != 8 {
            return Err(invalid(
                "positive unique native C3D8 contact cell identities required",
            ));
        }
        let corners = ids
            .iter()
            .map(|id| {
                indices
                    .get(id)
                    .copied()
                    .ok_or_else(|| invalid("native thermal cell has unknown node"))
            })
            .collect::<Result<Vec<_>>>()?;
        let lo: [u32; 3] =
            std::array::from_fn(|axis| corners.iter().map(|p| p[axis]).min().unwrap_or(n + 1));
        let expected: BTreeSet<_> = local
            .iter()
            .map(|p| std::array::from_fn(|axis| lo[axis] + p[axis]))
            .collect();
        if expected != corners.iter().copied().collect() || !covered.insert(lo) {
            return Err(invalid(
                "complete unique native thermal box cell coverage required",
            ));
        }
        let origin = nodes[&ids[0]];
        let edges: [[_; 3]; 3] = std::array::from_fn(|edge| {
            std::array::from_fn(|axis| nodes[&ids[[1, 3, 4][edge]]][axis] - origin[axis])
        });
        for (i, bits) in local.iter().enumerate() {
            for axis in 0..3 {
                let affine = origin[axis]
                    + (0..3)
                        .map(|edge| f64::from(bits[edge]) * edges[edge][axis])
                        .sum::<f64>();
                if (nodes[&ids[i]][axis] - affine).abs() > tolerance {
                    return Err(invalid("affine C3D8 thermal Jacobian map required"));
                }
            }
        }
        let det = edges[0][0] * (edges[1][1] * edges[2][2] - edges[1][2] * edges[2][1])
            - edges[0][1] * (edges[1][0] * edges[2][2] - edges[1][2] * edges[2][0])
            + edges[0][2] * (edges[1][0] * edges[2][1] - edges[1][1] * edges[2][0]);
        if !positive(det) || (det / (volume / f64::from(n).powi(3)) - 1.).abs() > 1e-10 {
            return Err(invalid(
                "positive exact native thermal cell volume required",
            ));
        }
        for id in ids {
            *measures
                .get_mut(id)
                .ok_or_else(|| invalid("native thermal cell node absent"))? += cell_capacity / 8.;
        }
    }
    let weights: Vec<_> = measures.values().copied().collect();
    let temperatures: Vec<_> = values.values().copied().collect();
    let total = weights.iter().sum::<f64>();
    let integral = weights
        .iter()
        .zip(&temperatures)
        .map(|(c, t)| c * t)
        .sum::<f64>();
    let mean = integral / total;
    let error = temperatures
        .iter()
        .map(|t| (t - mean).abs())
        .fold(0., f64::max);
    if !positive(total)
        || !positive(integral)
        || !positive(mean)
        || !error.is_finite()
        || (total - capacitance).abs() / capacitance > 1e-12
        || error > maximum_error_k
    {
        return Err(invalid(
            "native thermal projection exceeds approved loss or capacitance gate",
        ));
    }
    Ok(Projection {
        temperatures,
        capacitances: weights,
        mean_k: mean,
        capacitance_j_k: total,
        source_integral_j: integral,
        maximum_error_k: error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    #[test]
    fn transfer_request_requires_exact_time_explicit_loss_and_narrow_conservation() {
        let value = json!({"schema_version":1,"source_job":uuid::Uuid::new_v4().to_string(),"physical_time_s":120.,"destination":{"region":"lower","size_m":[0.02,0.01,0.01],"origin_m":[0.,0.,0.]},"maximum_projection_error_k":1.,"maximum_relative_conservation_error":1e-12});
        let request: ThermalProjectionRequest = serde_json::from_value(value.clone()).unwrap();
        request.validate().unwrap();
        for (key, replacement) in [
            ("schema_version", json!(2)),
            ("source_job", json!("invalid")),
            ("physical_time_s", json!(0)),
            ("maximum_projection_error_k", json!(-1)),
            ("maximum_projection_error_k", json!(1.01)),
            ("maximum_relative_conservation_error", json!(0)),
            ("maximum_relative_conservation_error", json!(1e-8)),
        ] {
            let mut changed = value.clone();
            changed[key] = replacement;
            assert!(
                serde_json::from_value::<ThermalProjectionRequest>(changed)
                    .unwrap()
                    .validate()
                    .is_err()
            );
        }
        for key in ["temperature_k", "density_kg_m3", "native_time_s"] {
            let mut changed = value.clone();
            changed[key] = json!(293.15);
            assert!(serde_json::from_value::<ThermalProjectionRequest>(changed).is_err());
        }
    }

    fn fixture() -> (Value, Value) {
        let id = |x: u32, y: u32, z: u32| z * 9 + y * 3 + x + 1;
        let mut nodes = std::collections::BTreeMap::new();
        let mut cells = std::collections::BTreeMap::new();
        let mut temperatures = std::collections::BTreeMap::new();
        for z in 0..3 {
            for y in 0..3 {
                for x in 0..3 {
                    let tag = id(x, y, z);
                    nodes.insert(
                        tag,
                        [f64::from(x) / 2., f64::from(y) / 2., f64::from(z) / 2.],
                    );
                    temperatures.insert(tag, 293. + f64::from(x) / 2.);
                }
            }
        }
        for z in 0..2 {
            for y in 0..2 {
                for x in 0..2 {
                    cells.insert(
                        z * 4 + y * 2 + x + 1,
                        vec![
                            id(x, y, z),
                            id(x + 1, y, z),
                            id(x + 1, y + 1, z),
                            id(x, y + 1, z),
                            id(x, y, z + 1),
                            id(x + 1, y, z + 1),
                            id(x + 1, y + 1, z + 1),
                            id(x, y + 1, z + 1),
                        ],
                    );
                }
            }
        }
        (
            json!({"schema_version":1,"coordinate_unit":"m","element_type":"C3D8","nodes":nodes,"elements":cells}),
            json!(temperatures),
        )
    }

    #[test]
    fn complete_native_linear_box_projection_conserves_capacitance_not_node_count() {
        let (mesh, temperatures) = fixture();
        let projected = project_box(&mesh, &temperatures, [1.; 3], 2, 1e-8, [2., 3.], 0.6).unwrap();
        assert_eq!(projected.temperatures.len(), 27);
        assert!((projected.mean_k - 293.5).abs() < 1e-12);
        assert!((projected.capacitance_j_k - 6.).abs() < 1e-12);
        assert!((projected.source_integral_j - 1761.).abs() < 1e-12);
        assert!((projected.maximum_error_k - 0.5).abs() < 1e-12);
        assert!(project_box(&mesh, &temperatures, [1.; 3], 2, 1e-8, [2., 3.], 0.49).is_err());
        // A corner contributes one eighth of a cell, not one twenty-seventh
        // of the volume. This catches ordinal/node-average implementations.
        let mut pulse = temperatures.clone();
        for value in pulse.as_object_mut().unwrap().values_mut() {
            *value = json!(293.);
        }
        pulse["1"] = json!(294.);
        let projected = project_box(&mesh, &pulse, [1.; 3], 2, 1e-8, [2., 3.], 1.).unwrap();
        assert!((projected.mean_k - 293. - 1. / 64.).abs() < 1e-12);
        assert!(project_box(&mesh, &pulse, [1.; 3], 2, 1e-8, [f64::MAX, 3.], 1.).is_err());
    }

    #[test]
    fn projection_rejects_missing_native_values_twisted_cells_and_geometry_drift() {
        let (mesh, temperatures) = fixture();
        let mut changed = temperatures.clone();
        changed.as_object_mut().unwrap().remove("27");
        assert!(project_box(&mesh, &changed, [1.; 3], 2, 1e-8, [2., 3.], 1.).is_err());
        changed = temperatures.clone();
        changed["1"] = json!(-1.);
        assert!(project_box(&mesh, &changed, [1.; 3], 2, 1e-8, [2., 3.], 1.).is_err());
        for altered in [
            "coordinate",
            "duplicated_cell",
            "twisted_cell",
            "outside_box",
        ] {
            let mut changed = mesh.clone();
            match altered {
                "coordinate" => changed["nodes"]["27"][2] = json!(1.001),
                "duplicated_cell" => changed["elements"]["8"] = changed["elements"]["1"].clone(),
                "twisted_cell" => changed["elements"]["1"][6] = json!(27),
                "outside_box" => changed["nodes"]["1"][0] = json!(-1.),
                _ => unreachable!(),
            }
            assert!(project_box(&changed, &temperatures, [1.; 3], 2, 1e-8, [2., 3.], 1.).is_err());
        }
    }
}
