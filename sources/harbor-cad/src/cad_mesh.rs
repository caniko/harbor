//! Source-bound imported mesh receipts and independent Cartesian correspondence.
use crate::{
    Result,
    cad_source::CadMeshDescriptor,
    contracts::*,
    qualification::NumericalEvidence,
    storage::{native_manifest, safe_path},
    worker::read_bounded,
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Mesh {
    schema_version: u32,
    synthetic: bool,
    coordinate_unit: String,
    nodes: BTreeMap<u64, [f64; 3]>,
    elements: BTreeMap<u64, [u64; 8]>,
    element_type: String,
    boundary_node_sets: BTreeMap<String, Vec<u64>>,
    semantic_face_selection: String,
    positive_gauss_jacobians: bool,
    integrated_volume_m3: f64,
}

pub fn verify_mesh(spec: &CadMeshDescriptor, value: Value) -> Result<f64> {
    spec.validate()?;
    let mesh: Mesh = serde_json::from_value(value)?;
    let n = spec.resolution;
    if mesh.schema_version != 1
        || mesh.synthetic != spec.synthetic
        || mesh.coordinate_unit != "m"
        || mesh.element_type != "C3D8"
        || !mesh.positive_gauss_jacobians
        || mesh.nodes.len() != (n + 1).pow(3) as usize
        || mesh.elements.len() != n.pow(3) as usize
        || mesh.semantic_face_selection
            != "planar bounding box plus area; no ordinal face dependency"
        || mesh.nodes.contains_key(&0)
        || mesh.elements.contains_key(&0)
    {
        return Err(invalid(
            "complete original-unit imported C3D8 mesh and topology evidence required",
        ));
    }
    let widths = spec.lengths_m();
    let mut positions = BTreeMap::new();
    let mut observed = BTreeSet::new();
    for (id, xyz) in &mesh.nodes {
        let mut ijk = [0u32; 3];
        for axis in 0..3 {
            let index =
                ((xyz[axis] - spec.bounds_m[2 * axis]) * f64::from(n) / widths[axis]).round();
            let expected = spec.bounds_m[2 * axis] + index * widths[axis] / f64::from(n);
            if !xyz[axis].is_finite()
                || !index.is_finite()
                || index < 0.
                || index > f64::from(n)
                || (xyz[axis] - expected).abs() > spec.geometry_tolerance_m
            {
                return Err(invalid(
                    "native nodes differ from the complete approved CAD world grid",
                ));
            }
            ijk[axis] = index as u32;
        }
        if !observed.insert(ijk) {
            return Err(invalid("duplicate native world-grid location"));
        }
        positions.insert(*id, ijk);
    }
    let mut origins = BTreeSet::new();
    let mut volume = 0.;
    for ids in mesh.elements.values() {
        if ids.iter().collect::<BTreeSet<_>>().len() != 8
            || ids.iter().any(|i| !positions.contains_key(i))
        {
            return Err(invalid(
                "eight unique registered nodes per native cell required",
            ));
        }
        let lower: [u32; 3] =
            std::array::from_fn(|axis| ids.iter().map(|id| positions[id][axis]).min().unwrap_or(n));
        let upper: [u32; 3] =
            std::array::from_fn(|axis| ids.iter().map(|id| positions[id][axis]).max().unwrap_or(n));
        if !origins.insert(lower)
            || (0..3).any(|axis| lower[axis] >= n || upper[axis] != lower[axis] + 1)
        {
            return Err(invalid(
                "complete nonoverlapping adjacent-cell coverage required",
            ));
        }
        // Eight-point C3D8 quadrature checks every node's orientation, including
        // twisted far corners that an edge-triple determinant cannot observe.
        let expected_jacobian = spec.volume_m3 / f64::from(n.pow(3)) / 8.;
        let g = 1. / 3f64.sqrt();
        for x in [-g, g] {
            for y in [-g, g] {
                for z in [-g, g] {
                    let mut jacobian = [[0.; 3]; 3];
                    for (id, [a, b, c]) in ids.iter().zip([
                        [-1., -1., -1.],
                        [1., -1., -1.],
                        [1., 1., -1.],
                        [-1., 1., -1.],
                        [-1., -1., 1.],
                        [1., -1., 1.],
                        [1., 1., 1.],
                        [-1., 1., 1.],
                    ]) {
                        let derivatives = [
                            a * (1. + b * y) * (1. + c * z) / 8.,
                            b * (1. + a * x) * (1. + c * z) / 8.,
                            c * (1. + a * x) * (1. + b * y) / 8.,
                        ];
                        for (axis, row) in jacobian.iter_mut().enumerate() {
                            for (j, entry) in row.iter_mut().enumerate() {
                                *entry += (mesh.nodes[id][axis] - mesh.nodes[&ids[0]][axis])
                                    * derivatives[j];
                            }
                        }
                    }
                    let [a, b, c] = jacobian;
                    let det = a[0] * (b[1] * c[2] - b[2] * c[1])
                        - a[1] * (b[0] * c[2] - b[2] * c[0])
                        + a[2] * (b[0] * c[1] - b[1] * c[0]);
                    if !det.is_finite()
                        || det <= 0.
                        || (det - expected_jacobian).abs()
                            > spec.volume_relative_tolerance * expected_jacobian
                    {
                        return Err(invalid(
                            "independent positive C3D8 Gauss Jacobian/volume failed",
                        ));
                    }
                    volume += det;
                }
            }
        }
    }
    if mesh.boundary_node_sets.len() != 6 {
        return Err(invalid("six semantic boundary planes required"));
    }
    for (axis, name) in ["x", "y", "z"].into_iter().enumerate() {
        for (suffix, index) in [("min", 0), ("max", n)] {
            let ids = mesh
                .boundary_node_sets
                .get(&format!("{name}{suffix}"))
                .ok_or_else(|| invalid("semantic CAD plane missing"))?;
            let expected: BTreeSet<_> = positions
                .iter()
                .filter(|(_, ijk)| ijk[axis] == index)
                .map(|(id, _)| *id)
                .collect();
            if ids.len() != (n + 1).pow(2) as usize
                || ids.iter().copied().collect::<BTreeSet<_>>() != expected
            {
                return Err(invalid("semantic CAD plane node association changed"));
            }
        }
    }
    let error = (volume - spec.volume_m3).abs() / spec.volume_m3;
    if !mesh.integrated_volume_m3.is_finite()
        || error > spec.volume_relative_tolerance
        || (mesh.integrated_volume_m3 - volume).abs()
            > spec.volume_relative_tolerance * spec.volume_m3
    {
        return Err(invalid(
            "native mesh volume differs from unchanged approved CAD volume gate",
        ));
    }
    Ok(error)
}

pub fn validate_receipt(plan: &ExecutionPlan, receipt: &Value) -> Result<()> {
    let spec = &plan
        .cad_source
        .as_ref()
        .ok_or_else(|| invalid("approved imported CAD source required"))?
        .geometry;
    spec.validate()?;
    let checks = &receipt["sandbox"]["checks"];
    if receipt["schema_version"] != 1
        || receipt["adapter"] != "Gmsh"
        || receipt["backend"] != "cpu"
        || receipt["executed"] != true
        || receipt["software_fallback"] != false
        || receipt["precision"] != "float64"
        || receipt["formulation"] != spec.formulation
        || receipt["request_sha256"] != digest(spec)?
        || receipt["brep_sha256"] != spec.brep_sha256
        || receipt["synthetic"] != spec.synthetic
        || receipt["region_name"] != spec.region_name
        || receipt["geometry_provenance"] != spec.geometry_provenance
        || receipt["source_transform"] != serde_json::to_value(spec.source_transform)?
        || receipt["world_bounds_m"] != serde_json::to_value(spec.bounds_m)?
        || receipt["cad_volume_m3"] != spec.volume_m3
        || receipt["source_unit"] != "mm"
        || receipt["placement_translation_unit"] != "mm"
        || receipt["coordinate_unit"] != "m"
        || receipt["scale_to_m"] != 0.001
        || receipt["gap_healing"] != false
        || receipt["physical_validation"] != "unqualified"
        || receipt["gmsh_version"] != "4.15.2"
        || receipt["gmsh_source_sha256"]
            != "be3f66f225d27ba9fa014f07e83169285da8a051b0e8ab7103d88066b39bdd3e"
        || receipt["nodes"] != u64::from((spec.resolution + 1).pow(3))
        || receipt["elements"] != u64::from(spec.resolution.pow(3))
        || receipt["sandbox"]["policy"] != crate::execution::CAD_MESH_SANDBOX_POLICY
        || [
            "operation_closure_only",
            "no_gpu_nodes",
            "no_sysfs",
            "no_host_home",
            "no_session_bus",
            "no_worker_socket",
            "network_namespace_isolated",
            "descriptor_readonly",
            "source_brep_readonly",
            "named_source_only",
        ]
        .into_iter()
        .any(|k| checks[k] != true)
    {
        return Err(invalid(
            "imported CAD mesh receipt differs from approved source, topology, units or isolated policy",
        ));
    }
    Ok(())
}

pub fn verify_receipt(
    plan: &ExecutionPlan,
    root: &Path,
    receipt: &Value,
) -> Result<NumericalEvidence> {
    validate_receipt(plan, receipt)?;
    let spec = &plan
        .cad_source
        .as_ref()
        .ok_or_else(|| invalid("imported CAD source required"))?
        .geometry;
    let record = native_manifest(
        root,
        "mesh.json",
        32 * 1024 * 1024,
        "verify closed native mesh",
    )?;
    if receipt["mesh_sha256"] != record.sha256 {
        return Err(invalid("native mesh bytes differ from exact receipt"));
    }
    let raw = read_bounded(&safe_path(root, "mesh.json")?, 32 * 1024 * 1024)?;
    let error = verify_mesh(spec, serde_json::from_slice(&raw)?)?;
    Ok(NumericalEvidence {
        reference: "original approved CAD solid volume and complete Cartesian world grid".into(),
        scope: "geometric correspondence only; no solver, numerical field or physical validation"
            .into(),
        error_kind: "relative_volume_error".into(),
        error,
        tolerance: spec.volume_relative_tolerance,
    })
}
