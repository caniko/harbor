//! Lossless VTK XML control-point view of closed authoritative cooling CSVs.
use crate::{
    Result,
    contracts::{ArtifactManifest, ExecutionPlan, invalid},
    cooling_fields::{self, Node},
    storage::{Store, commit_artifact, safe_path},
};
use std::fmt::Write;

fn xml(spec: &crate::cooling_execution::CoolingExecutionSpec, step: u64, nodes: &[Node]) -> String {
    let q = spec.request.spatial_refinement as usize;
    let [nx, ny] = spec.prepared.retained.extrusion.source_grid_shape;
    let extent = format!("0 {} 0 {} 0 0", nx * q - 1, (ny - 2) * q - 1);
    let depth = spec.prepared.retained.extrusion.extrusion_m;
    let z = spec.request.initialization.retained.destination_origin_m[2];
    let spacing = spec.prepared.retained.extrusion.spacing_m / q as f64;
    let factor = (q * q) as u64 * u64::from(spec.request.integration_substeps);
    let time = (step / factor) as f64 * spec.prepared.initialization.thermal_physical_step_s;
    let mut output = format!(
        "<?xml version=\"1.0\"?>\n<VTKFile type=\"StructuredGrid\" version=\"1.0\" byte_order=\"LittleEndian\"><StructuredGrid WholeExtent=\"{extent}\"><FieldData>"
    );
    for (name, value, unit) in [
        ("physical_time_s", time, "s"),
        ("extrusion_m", depth, "m"),
        ("subcontrol_volume_m3", spacing * spacing * depth, "m3"),
    ] {
        write!(output,"<DataArray Name=\"{name}\" type=\"Float64\" NumberOfComponents=\"1\" NumberOfTuples=\"1\" format=\"ascii\" unit=\"{unit}\">{value}</DataArray>").expect("String writes are infallible");
    }
    write!(output,"</FieldData><Piece Extent=\"{extent}\"><Points><DataArray type=\"Float64\" NumberOfComponents=\"3\" format=\"ascii\" unit=\"m\">").expect("String writes are infallible");
    for node in nodes {
        write!(output, "{} {} {} ", node.xy[0], node.xy[1], z)
            .expect("String writes are infallible");
    }
    output.push_str("</DataArray></Points><PointData>");
    for (index, (name, kind, unit)) in [
        ("i", "Int32", "1"),
        ("j", "Int32", "1"),
        ("parent_i", "Int32", "1"),
        ("parent_j", "Int32", "1"),
        ("water_fraction", "Float64", "1"),
        ("specific_enthalpy_j_kg", "Float64", "J/kg"),
        ("temperature_k", "Float64", "K"),
        ("liquid_fraction", "Float64", "1"),
    ]
    .into_iter()
    .enumerate()
    {
        write!(output,"<DataArray Name=\"{name}\" type=\"{kind}\" NumberOfComponents=\"1\" format=\"ascii\" unit=\"{unit}\">").expect("String writes are infallible");
        for node in nodes {
            let value = match index {
                0 => node.i as f64,
                1 => node.j as f64,
                2 => node.parent[0] as f64,
                3 => node.parent[1] as f64,
                4 => node.water_fraction,
                5 => node.enthalpy,
                6 => node.temperature,
                _ => node.liquid_fraction,
            };
            write!(output, "{value} ").expect("String writes are infallible");
        }
        output.push_str("</DataArray>");
    }
    output.push_str("</PointData><CellData/></Piece></StructuredGrid></VTKFile>\n");
    output
}

/// Original nodal/control association remains explicit: this is not a cell mesh.
pub(crate) fn publish(
    store: &Store,
    id: &str,
    plan: &ExecutionPlan,
    native_root: &std::path::Path,
) -> Result<()> {
    let Some(spec) = &plan.retained_cooling else {
        return Ok(());
    };
    let source = crate::cooling_execution::registered(store, id, plan)?;
    let original = crate::worker::read_bounded(&source, 16 * 1024 * 1024)?;
    for step in spec.request.observation_steps() {
        let raw = crate::worker::read_bounded(
            &safe_path(native_root, &format!("cooling-{step}.csv"))?,
            64 * 1024 * 1024,
        )?;
        let nodes = cooling_fields::nodes(spec, &original, step, &raw)?;
        let output = xml(spec, step, &nodes);
        commit_artifact(
            native_root,
            &format!("cooling-{step}.vts"),
            output.as_bytes(),
            "vts",
            "lossless derived VTK XML view of closed original Float64 cooling control points; explicit original IDs, SI coordinates, units, time and extrusion; no interpolated cells",
        )?;
    }
    Ok(())
}

pub(crate) fn verify(store: &Store, id: &str, plan: &ExecutionPlan, source: &[u8]) -> Result<()> {
    let spec = plan
        .retained_cooling
        .as_ref()
        .ok_or_else(|| invalid("source-bound cooling VTK approval required"))?;
    let root = store.job_dir(id)?;
    let mut published = 0;
    for step in spec.request.observation_steps() {
        if store
            .artifact_record(id, &format!("stages/retained-cooling/cooling-{step}.vts"))?
            .is_some()
        {
            published += 1;
        }
    }
    if published == 0 {
        return Ok(());
    }
    if published != spec.request.observation_base_steps.len() {
        return Err(invalid("complete declared cooling VTK history required"));
    }
    for (step, time) in spec
        .request
        .observation_steps()
        .into_iter()
        .zip(spec.times_s())
    {
        let path = format!("stages/retained-cooling/cooling-{step}.vts");
        let record = store
            .artifact_record(id, &path)?
            .ok_or_else(|| invalid("registered cooling VTK view missing"))?;
        if record.format != "vts"
            || record.bytes == 0
            || record.bytes > 64 * 1024 * 1024
            || record.time_s != Some(time)
            || record.association.as_deref() != Some(cooling_fields::ASSOCIATION)
            || record.units.as_deref() != Some(cooling_fields::FIELD_UNITS)
        {
            return Err(invalid(
                "registered cooling VTK units/time/original-point association changed",
            ));
        }
        let raw = crate::worker::read_bounded(
            &safe_path(
                &root,
                &format!("stages/retained-cooling/cooling-{step}.csv"),
            )?,
            64 * 1024 * 1024,
        )?;
        let nodes = cooling_fields::nodes(spec, source, step, &raw)?;
        let expected = xml(spec, step, &nodes);
        let observed = crate::worker::read_bounded(&safe_path(&root, &path)?, record.bytes)?;
        use sha2::{Digest, Sha256};
        if observed != expected.as_bytes()
            || observed.len() as u64 != record.bytes
            || format!("{:x}", Sha256::digest(&observed)) != record.sha256
        {
            return Err(invalid(
                "lossless VTK cooling view differs from complete original CSV controls",
            ));
        }
    }
    Ok(())
}

pub(crate) fn annotate(plan: &ExecutionPlan, artifacts: &mut [ArtifactManifest]) -> Result<()> {
    let Some(spec) = &plan.retained_cooling else {
        return Ok(());
    };
    for (step, time) in spec
        .request
        .observation_steps()
        .into_iter()
        .zip(spec.times_s())
    {
        let path = format!("stages/retained-cooling/cooling-{step}.vts");
        let records = artifacts
            .iter_mut()
            .filter(|r| r.path == path)
            .collect::<Vec<_>>();
        if records.len() != 1 {
            return Err(invalid(
                "every approved cooling time requires one complete lossless VTK view",
            ));
        }
        for record in records {
            record.time_s = Some(time);
            record.association = Some(cooling_fields::ASSOCIATION.into());
            record.units = Some(cooling_fields::FIELD_UNITS.into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_original_control_points_roundtrip_float64_topology_identity_units_and_association()
     {
        let (_, spec, _) = crate::cooling_execution_tests::fixture();
        let nx = spec.prepared.retained.extrusion.source_grid_shape[0] * 2;
        let height = (spec.prepared.retained.extrusion.source_grid_shape[1] - 2) * 2;
        let nodes = (0..nx * height)
            .map(|k| Node {
                i: k % nx,
                j: k / nx + 1,
                parent: [(k % nx) / 2, (k / nx) / 2 + 1],
                xy: [-5.293956e-23, k as f64 * 1e-8],
                water_fraction: f64::from_bits(0.5f64.to_bits() + 1),
                enthalpy: 2.123456789012345e5,
                temperature: 273.1499999999999,
                liquid_fraction: 1.,
            })
            .collect::<Vec<_>>();
        let xml = xml(&spec, 0, &nodes);
        let document = roxmltree::Document::parse(&xml).unwrap();
        let root = document.root_element();
        assert_eq!(root.attribute("type"), Some("StructuredGrid"));
        let piece = root
            .descendants()
            .find(|n| n.has_tag_name("Piece"))
            .unwrap();
        assert_eq!(
            piece.attribute("Extent"),
            Some(format!("0 {} 0 {} 0 0", nx - 1, height - 1).as_str())
        );
        let points = piece
            .children()
            .find(|n| n.has_tag_name("Points"))
            .unwrap()
            .children()
            .find(|n| n.is_element())
            .unwrap();
        assert_eq!(points.attribute("unit"), Some("m"));
        let values = points
            .text()
            .unwrap()
            .split_whitespace()
            .map(|s| s.parse::<f64>().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(values.len(), nodes.len() * 3);
        for (xyz, node) in values.as_chunks::<3>().0.iter().zip(&nodes) {
            assert_eq!(xyz[..2], node.xy);
            assert_eq!(
                xyz[2],
                spec.request.initialization.retained.destination_origin_m[2]
            );
        }
        let arrays = piece
            .children()
            .find(|n| n.has_tag_name("PointData"))
            .unwrap()
            .children()
            .filter(|n| n.is_element())
            .collect::<Vec<_>>();
        assert_eq!(arrays.len(), 8);
        for array in arrays {
            let name = array.attribute("Name").unwrap();
            let values = array
                .text()
                .unwrap()
                .split_whitespace()
                .map(|s| s.parse::<f64>().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(values.len(), nodes.len());
            for (v, node) in values.into_iter().zip(&nodes) {
                assert_eq!(
                    v,
                    match name {
                        "i" => node.i as f64,
                        "j" => node.j as f64,
                        "parent_i" => node.parent[0] as f64,
                        "parent_j" => node.parent[1] as f64,
                        "water_fraction" => node.water_fraction,
                        "specific_enthalpy_j_kg" => node.enthalpy,
                        "temperature_k" => node.temperature,
                        "liquid_fraction" => node.liquid_fraction,
                        _ => panic!("undeclared field"),
                    }
                );
            }
            assert_eq!(
                array.attribute("unit"),
                Some(match name {
                    "temperature_k" => "K",
                    "specific_enthalpy_j_kg" => "J/kg",
                    _ => "1",
                })
            );
        }
        assert!(
            piece
                .children()
                .find(|n| n.has_tag_name("CellData"))
                .unwrap()
                .children()
                .all(|n| !n.is_element())
        );
    }
}
