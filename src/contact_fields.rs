//! Independent original DAT coverage, planar mesh and series-compliance checks.
use crate::{Result, contact::ContactReferenceSpec, contracts::invalid};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

type Values = BTreeMap<Vec<u32>, Vec<f64>>;
type Fields = BTreeMap<String, Vec<(u32, Values)>>;

fn finite(value: &Value) -> Result<f64> {
    value
        .as_f64()
        .filter(|v| v.is_finite())
        .ok_or_else(|| invalid("finite contact field component required"))
}

pub(crate) fn parse_dat(text: &str) -> Result<Value> {
    let mut fields = Fields::new();
    let mut current: Option<(String, usize, usize)> = None;
    for line in text.lines().map(str::trim).filter(|v| !v.is_empty()) {
        if line.starts_with("S T E P") || line.starts_with("INCREMENT") {
            current = None;
            continue;
        }
        if let Some((label, rest)) = line.split_once(" for set ") {
            let (name, ids, components, set) = match label {
                "displacements (vx,vy,vz)" => ("displacement", 1, 3, "NALL"),
                "forces (fx,fy,fz)" => ("reaction_force", 1, 3, "NALL"),
                "stresses (elem, integ.pnt.,sxx,syy,szz,sxy,sxz,syz)" => ("stress", 2, 6, "EALL"),
                _ => return Err(invalid("unrelated native contact DAT field")),
            };
            let (native_set, stamp) = rest
                .split_once("and time")
                .ok_or_else(|| invalid("native contact state required"))?;
            let time: f64 = stamp
                .trim()
                .replace('D', "E")
                .parse()
                .map_err(|_| invalid("native contact state"))?;
            if native_set.trim() != set || ![1., 2.].contains(&time) {
                return Err(invalid(
                    "exact final static preload/final native state required",
                ));
            }
            let snapshots = fields.entry(name.into()).or_default();
            if snapshots.iter().any(|(s, _)| *s == time as u32) {
                return Err(invalid("duplicate contact native state"));
            }
            snapshots.push((time as u32, Values::new()));
            current = Some((name.into(), ids, components));
        } else {
            let (name, ids, components) = current
                .as_ref()
                .ok_or_else(|| invalid("contact DAT data outside field"))?;
            let parts: Vec<_> = line.split_whitespace().collect();
            if parts.len() != ids + components {
                return Err(invalid("complete native contact components required"));
            }
            let key = parts[..*ids]
                .iter()
                .map(|v| {
                    v.parse::<u32>()
                        .ok()
                        .filter(|v| *v > 0)
                        .ok_or_else(|| invalid("native positive contact identifier required"))
                })
                .collect::<Result<Vec<_>>>()?;
            let values = parts[*ids..]
                .iter()
                .map(|v| {
                    v.replace('D', "E")
                        .parse::<f64>()
                        .ok()
                        .filter(|v| v.is_finite())
                        .ok_or_else(|| invalid("finite original contact component required"))
                })
                .collect::<Result<Vec<_>>>()?;
            let snapshot = fields
                .get_mut(name)
                .and_then(|v| v.last_mut())
                .ok_or_else(|| invalid("native contact state missing"))?;
            if snapshot.1.insert(key, values).is_some() {
                return Err(invalid("duplicate contact native entity"));
            }
        }
    }
    if fields.len() != 3
        || fields
            .values()
            .any(|v| v.iter().map(|(s, _)| *s).collect::<Vec<_>>() != [1, 2])
    {
        return Err(invalid(
            "complete two-state native contact schedules required",
        ));
    }
    let serialized = fields.into_iter().map(|(name,snapshots)| (name, snapshots.into_iter().map(|(time,values)| json!({"solver_step_parameter":f64::from(time),"physical_time_s":null,"values":values.into_iter().map(|(id,value)| json!({"id":id,"value":value})).collect::<Vec<_>>()})).collect::<Vec<_>>())).collect::<BTreeMap<_,_>>();
    Ok(
        json!({"schema_version":1,"static":true,"coordinate_unit":"m","fields":serialized,"units":{"displacement":"m","reaction_force":"N","stress":"Pa"}}),
    )
}

fn entity_map(
    value: &Value,
    nodes: &BTreeMap<u32, [f64; 3]>,
    cells: &BTreeMap<u32, Vec<u32>>,
    field: &str,
) -> Result<Values> {
    let values = value
        .as_array()
        .ok_or_else(|| invalid("native contact values required"))?;
    let mut map = Values::new();
    for item in values {
        let id = item["id"]
            .as_array()
            .ok_or_else(|| invalid("typed native contact identity required"))?
            .iter()
            .map(|v| {
                v.as_u64()
                    .and_then(|v| u32::try_from(v).ok())
                    .filter(|v| *v > 0)
                    .ok_or_else(|| invalid("contact native ID"))
            })
            .collect::<Result<Vec<_>>>()?;
        let row = item["value"]
            .as_array()
            .ok_or_else(|| invalid("native contact components"))?
            .iter()
            .map(finite)
            .collect::<Result<Vec<_>>>()?;
        if map.insert(id, row).is_some() {
            return Err(invalid("duplicate contact entity"));
        }
    }
    let stress = field == "stress";
    let expected: BTreeSet<Vec<u32>> = if stress {
        cells
            .keys()
            .flat_map(|tag| (1..=8).map(move |ip| vec![*tag, ip]))
            .collect()
    } else {
        nodes.keys().map(|tag| vec![*tag]).collect()
    };
    if map.keys().cloned().collect::<BTreeSet<_>>() != expected
        || map.values().any(|v| v.len() != if stress { 6 } else { 3 })
    {
        return Err(invalid(
            "complete original contact node/integration-point components required",
        ));
    }
    Ok(map)
}

pub(crate) fn assess(spec: &ContactReferenceSpec, mesh: &Value, fields: &Value) -> Result<Value> {
    spec.validate()?;
    let nodes: BTreeMap<u32, [f64; 3]> = serde_json::from_value(mesh["nodes"].clone())?;
    let cells: BTreeMap<u32, Vec<u32>> = serde_json::from_value(mesh["elements"].clone())?;
    let sets: BTreeMap<String, Vec<u32>> =
        serde_json::from_value(mesh["boundary_node_sets"].clone())?;
    let n = spec.resolution;
    let count = (n + 1).pow(3);
    let offset = mesh["block_element_offset"]
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| invalid("original contact block element offset required"))?;
    if mesh["schema_version"] != 1
        || mesh["synthetic"] != true
        || mesh["coordinate_unit"] != "m"
        || mesh["element_type"] != "C3D8"
        || mesh["initial_gap_m"].as_f64() != Some(spec.initial_gap_m)
        || mesh["block_node_offset"] != count
        || nodes.len() != (2 * count) as usize
        || cells.len() != (2 * n.pow(3)) as usize
        || fields["schema_version"] != 1
        || fields["static"] != true
        || fields["coordinate_unit"] != "m"
        || fields["units"] != json!({"displacement":"m","reaction_force":"N","stress":"Pa"})
        || fields["fields"].as_object().is_none_or(|v| v.len() != 3)
    {
        return Err(invalid(
            "exact synthetic two-block native SI contact mesh/field metadata required",
        ));
    }
    let mut grid = BTreeMap::new();
    let mut by_tag = BTreeMap::new();
    for (&tag, xyz) in &nodes {
        if tag == 0 || tag > 2 * count || xyz.iter().any(|v| !v.is_finite()) {
            return Err(invalid("complete finite contact nodes required"));
        }
        let block = u32::from(tag > count);
        let origin = if block == 0 {
            0.
        } else {
            spec.size_m[2] + spec.initial_gap_m
        };
        let mut index = [0u32; 3];
        for axis in 0..3 {
            let coordinate = xyz[axis] - if axis == 2 { origin } else { 0. };
            let tick = (coordinate / spec.size_m[axis] * f64::from(n)).round();
            if !(0. ..=f64::from(n)).contains(&tick)
                || (coordinate - tick * spec.size_m[axis] / f64::from(n)).abs()
                    > spec.geometry_tolerance_m
            {
                return Err(invalid(
                    "native contact coordinates differ from approved exact box and gap",
                ));
            }
            index[axis] = tick as u32;
        }
        if grid.insert((block, index), tag).is_some() {
            return Err(invalid("duplicated native contact geometry"));
        }
        by_tag.insert(tag, index);
        if block == 1 {
            let original = nodes
                .get(&(tag - count))
                .ok_or_else(|| invalid("separate contact block correspondence required"))?;
            for axis in 0..3 {
                let expected = original[axis] + if axis == 2 { origin } else { 0. };
                if (xyz[axis] - expected).abs() > spec.geometry_tolerance_m {
                    return Err(invalid("contact mesh translation/gap changed"));
                }
            }
        }
    }
    let mut cubes = BTreeSet::new();
    for (&tag, ids) in &cells {
        let block = u32::from(tag > offset);
        if tag == 0 || ids.len() != 8 || ids.iter().copied().collect::<BTreeSet<_>>().len() != 8 {
            return Err(invalid(
                "complete original C3D8 contact connectivity required",
            ));
        }
        let corners = ids
            .iter()
            .map(|id| {
                by_tag
                    .get(id)
                    .copied()
                    .ok_or_else(|| invalid("contact element has unknown node"))
            })
            .collect::<Result<Vec<_>>>()?;
        if ids.iter().any(|id| u32::from(*id > count) != block) {
            return Err(invalid("contact elements cross separate blocks"));
        }
        let lo: [u32; 3] =
            std::array::from_fn(|a| corners.iter().map(|p| p[a]).min().unwrap_or(n + 1));
        let expected: BTreeSet<_> = (0..8)
            .map(|i| {
                [
                    lo[0] + (i & 1),
                    lo[1] + ((i >> 1) & 1),
                    lo[2] + ((i >> 2) & 1),
                ]
            })
            .collect();
        if expected != corners.iter().copied().collect() || !cubes.insert((block, lo)) {
            return Err(invalid("native C3D8 contact volume coverage changed"));
        }
        let a = nodes
            .get(&ids[0])
            .ok_or_else(|| invalid("contact corner"))?;
        let edges: Vec<[f64; 3]> = [1, 3, 4]
            .iter()
            .map(|i| std::array::from_fn(|axis| nodes[&ids[*i]][axis] - a[axis]))
            .collect();
        // All eight local corners must be the same affine positive C3D8 map;
        // checking only a single corner determinant could miss a twisted cell.
        for (local, bits) in [
            [0, 0, 0],
            [1, 0, 0],
            [1, 1, 0],
            [0, 1, 0],
            [0, 0, 1],
            [1, 0, 1],
            [1, 1, 1],
            [0, 1, 1],
        ]
        .iter()
        .enumerate()
        {
            for axis in 0..3 {
                let expected = a[axis]
                    + (0..3)
                        .map(|edge| f64::from(bits[edge]) * edges[edge][axis])
                        .sum::<f64>();
                if (nodes[&ids[local]][axis] - expected).abs() > spec.geometry_tolerance_m {
                    return Err(invalid(
                        "native contact C3D8 local ordering/Jacobian map changed",
                    ));
                }
            }
        }
        let determinant = edges[0][0] * (edges[1][1] * edges[2][2] - edges[1][2] * edges[2][1])
            - edges[0][1] * (edges[1][0] * edges[2][2] - edges[1][2] * edges[2][0])
            + edges[0][2] * (edges[1][0] * edges[2][1] - edges[1][1] * edges[2][0]);
        if !determinant.is_finite()
            || determinant <= 0.
            || (determinant / (spec.size_m.iter().product::<f64>() / f64::from(n).powi(3)) - 1.)
                .abs()
                > 1e-10
        {
            return Err(invalid("positive exact native contact volume required"));
        }
        if block == 1
            && cells
                .get(&(tag - offset))
                .is_none_or(|lower| lower.iter().map(|v| *v + count).collect::<Vec<_>>() != *ids)
        {
            return Err(invalid("contact block cell correspondence changed"));
        }
    }
    for (name, block, z) in [
        ("bottom", 0, 0),
        ("top", 1, n),
        ("lower_interface", 0, n),
        ("upper_interface", 1, 0),
    ] {
        let expected = grid
            .iter()
            .filter(|((b, p), _)| *b == block && p[2] == z)
            .map(|(_, id)| *id)
            .collect::<BTreeSet<_>>();
        let actual = sets
            .get(name)
            .ok_or_else(|| invalid("native contact semantic set required"))?;
        if actual.len() != ((n + 1).pow(2)) as usize
            || actual.iter().copied().collect::<BTreeSet<_>>() != expected
        {
            return Err(invalid(
                "complete geometric contact semantic surface required",
            ));
        }
    }
    let area = spec.size_m[0] * spec.size_m[1];
    let mut checks = Vec::new();
    for state in 1..=2 {
        let mut native = BTreeMap::new();
        for name in ["displacement", "reaction_force", "stress"] {
            let snapshots = fields["fields"][name]
                .as_array()
                .filter(|v| v.len() == 2)
                .ok_or_else(|| invalid("two static contact states required"))?;
            if snapshots.iter().enumerate().any(|(i, s)| {
                s["solver_step_parameter"].as_f64() != Some((i + 1) as f64)
                    || !s["physical_time_s"].is_null()
            }) {
                return Err(invalid("contact static states cannot infer physical time"));
            }
            native.insert(
                name,
                entity_map(
                    &snapshots[(state - 1) as usize]["values"],
                    &nodes,
                    &cells,
                    name,
                )?,
            );
        }
        let expected = spec.reference(state)?;
        let p = expected.pressure_pa;
        let h = spec.size_m[2];
        let length_scale = expected
            .compression_m
            .max(
                h * expected
                    .thermal_strains
                    .iter()
                    .map(|v| v.abs())
                    .fold(0., f64::max),
            )
            .max(spec.geometry_tolerance_m);
        let pressure_scale =
            p.max(spec.young_modulus_pa.iter().copied().fold(0., f64::max) * length_scale / h);
        let mut displacement: f64 = 0.;
        for (id, row) in &native["displacement"] {
            let tag = id[0];
            let block = usize::from(tag > count);
            let z = nodes[&tag][2];
            let slope = expected.thermal_strains[block] - p / spec.young_modulus_pa[block];
            let dz = if block == 0 {
                slope * z
            } else {
                -expected.compression_m - slope * (2. * h + spec.initial_gap_m - z)
            };
            displacement = displacement
                .max(row[0].abs().max(row[1].abs()).max((row[2] - dz).abs()) / length_scale);
        }
        let mut stress: f64 = 0.;
        for (id, row) in &native["stress"] {
            let block = usize::from(id[0] > offset);
            let transverse = -spec.young_modulus_pa[block] * expected.thermal_strains[block];
            let target = [transverse, transverse, -p, 0., 0., 0.];
            stress = stress.max(
                row.iter()
                    .zip(target)
                    .map(|(a, b)| (a - b).abs() / pressure_scale)
                    .fold(0., f64::max),
            );
        }
        let forces = &native["reaction_force"];
        let sum = |name: &str| {
            sets[name]
                .iter()
                .map(|tag| forces[&vec![*tag]][2])
                .sum::<f64>()
        };
        let force_error = (sum("bottom") - p * area)
            .abs()
            .max((sum("top") + p * area).abs())
            .max(forces.values().map(|v| v[2]).sum::<f64>().abs())
            / (pressure_scale * area);
        let mut gap_error: f64 = 0.;
        for x in 0..=n {
            for y in 0..=n {
                let lower = grid[&(0, [x, y, n])];
                let upper = grid[&(1, [x, y, 0])];
                let gap = spec.initial_gap_m + native["displacement"][&vec![upper]][2]
                    - native["displacement"][&vec![lower]][2];
                gap_error = gap_error.max((gap - expected.gap_m).abs() / length_scale);
            }
        }
        let errors = json!({"displacement":displacement,"stress":stress,"reaction_force_balance":force_error,"gap":gap_error});
        if [displacement, stress, force_error, gap_error]
            .iter()
            .any(|v| !v.is_finite() || *v > spec.numerical_tolerance)
        {
            return Err(invalid(format!(
                "unchanged original contact native gate failed: {errors}"
            )));
        }
        checks.push(json!({"reference":expected,"normalized_errors":errors,"tolerance":spec.numerical_tolerance,"passed":true}));
    }
    Ok(json!(checks))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (ContactReferenceSpec, Value, Value) {
        let spec:ContactReferenceSpec=serde_json::from_value(json!({"schema_version":1,"synthetic":true,"backend":"cpu","formulation":"planar_linear_penalty_contact","size_m":[0.001,0.001,0.001],"resolution":2,"geometry_tolerance_m":1e-8,"initial_gap_m":0.,"preload_compression_m":0.5e-6,"final_compression_m":1e-6,"young_modulus_pa":[1e8,1e8],"expansion_per_k":[1e-5,1e-5],"reference_temperature_k":293.15,"final_temperatures_k":[293.15,293.15],"contact_stiffness_pa_m":1e12,"numerical_tolerance":0.002,"material_provenance":"synthetic elastic material","contact_provenance":"synthetic planar law","boundary_provenance":"explicit fixed transverse and top displacement"})).unwrap();
        let id = |b: u32, x: u32, y: u32, z: u32| b * 27 + z * 9 + y * 3 + x + 1;
        let mut nodes = BTreeMap::new();
        let mut cells = BTreeMap::new();
        let mut sets: BTreeMap<&str, Vec<u32>> = [
            ("bottom", vec![]),
            ("top", vec![]),
            ("lower_interface", vec![]),
            ("upper_interface", vec![]),
        ]
        .into_iter()
        .collect();
        for b in 0..2 {
            for z in 0..3 {
                for y in 0..3 {
                    for x in 0..3 {
                        let tag = id(b, x, y, z);
                        nodes.insert(
                            tag,
                            [
                                f64::from(x) * 0.0005,
                                f64::from(y) * 0.0005,
                                f64::from(z) * 0.0005 + f64::from(b) * 0.001,
                            ],
                        );
                        if z == 0 {
                            sets.get_mut(if b == 0 { "bottom" } else { "upper_interface" })
                                .unwrap()
                                .push(tag);
                        }
                        if z == 2 {
                            sets.get_mut(if b == 0 { "lower_interface" } else { "top" })
                                .unwrap()
                                .push(tag);
                        }
                    }
                }
            }
            for z in 0..2 {
                for y in 0..2 {
                    for x in 0..2 {
                        cells.insert(
                            b * 8 + z * 4 + y * 2 + x + 1,
                            vec![
                                id(b, x, y, z),
                                id(b, x + 1, y, z),
                                id(b, x + 1, y + 1, z),
                                id(b, x, y + 1, z),
                                id(b, x, y, z + 1),
                                id(b, x + 1, y, z + 1),
                                id(b, x + 1, y + 1, z + 1),
                                id(b, x, y + 1, z + 1),
                            ],
                        );
                    }
                }
            }
        }
        let mesh = json!({"schema_version":1,"synthetic":true,"coordinate_unit":"m","nodes":nodes,"elements":cells,"element_type":"C3D8","boundary_node_sets":sets,"initial_gap_m":0.,"block_node_offset":27,"block_element_offset":8});
        let mut text = String::new();
        for state in 1..=2 {
            let reference = spec.reference(state).unwrap();
            let p = reference.pressure_pa;
            text += &format!("displacements (vx,vy,vz) for set NALL and time {state}\n");
            for (tag, xyz) in &nodes {
                let dz = if *tag <= 27 {
                    -p / 1e8 * xyz[2]
                } else {
                    -reference.compression_m + p / 1e8 * (0.002 - xyz[2])
                };
                text += &format!("{tag} 0 0 {dz:.17e}\n");
            }
            text += &format!("forces (fx,fy,fz) for set NALL and time {state}\n");
            for tag in nodes.keys() {
                let force = if sets["bottom"].contains(tag) {
                    p * 1e-6 / 9.
                } else if sets["top"].contains(tag) {
                    -p * 1e-6 / 9.
                } else {
                    0.
                };
                text += &format!("{tag} 0 0 {force:.17e}\n");
            }
            text += &format!(
                "stresses (elem, integ.pnt.,sxx,syy,szz,sxy,sxz,syz) for set EALL and time {state}\n"
            );
            for tag in cells.keys() {
                for ip in 1..=8 {
                    text += &format!("{tag} {ip} 0 0 {:.17e} 0 0 0\n", -p);
                }
            }
        }
        (spec, mesh, parse_dat(&text).unwrap())
    }
    #[test]
    fn independent_contact_geometry_force_gap_and_original_field_gates_reject_drift() {
        let (spec, mesh, fields) = fixture();
        assert_eq!(
            assess(&spec, &mesh, &fields)
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            2
        );
        for key in ["displacement", "reaction_force", "stress"] {
            let mut changed = fields.clone();
            changed["fields"][key][1]["values"]
                .as_array_mut()
                .unwrap()
                .remove(0);
            assert!(assess(&spec, &mesh, &changed).is_err());
            changed = fields.clone();
            changed["fields"][key][1]["values"][0]["value"][2] = json!(99.);
            assert!(assess(&spec, &mesh, &changed).is_err());
        }
        for key in ["initial_gap_m", "block_node_offset", "element_type"] {
            let mut changed = mesh.clone();
            changed[key] = json!(1.);
            assert!(assess(&spec, &changed, &fields).is_err());
        }
        let mut changed = mesh.clone();
        changed["nodes"]["28"][2] = json!(0.001001);
        assert!(assess(&spec, &changed, &fields).is_err());
        changed = mesh.clone();
        changed["elements"]["1"][0] = changed["elements"]["1"][1].clone();
        assert!(assess(&spec, &changed, &fields).is_err());
        changed = mesh.clone();
        changed["boundary_node_sets"]["bottom"][0] = json!(27);
        assert!(assess(&spec, &changed, &fields).is_err());
    }
    #[test]
    fn original_contact_parser_requires_both_complete_states_without_invented_time() {
        let mut text = String::new();
        for time in [1, 2] {
            text += &format!(
                "displacements (vx,vy,vz) for set NALL and time {time}\n1 0 0 -1D-6\nforces (fx,fy,fz) for set NALL and time {time}\n1 0 0 -1D-2\nstresses (elem, integ.pnt.,sxx,syy,szz,sxy,sxz,syz) for set EALL and time {time}\n1 1 0 0 -1D3 0 0 0\n"
            );
        }
        let fields = parse_dat(&text).unwrap();
        assert_eq!(
            fields["fields"]["displacement"][0]["solver_step_parameter"],
            json!(1.0)
        );
        assert_eq!(
            fields["fields"]["displacement"][1]["solver_step_parameter"],
            json!(2.0)
        );
        assert_eq!(
            fields["fields"]["reaction_force"][1]["values"][0]["value"],
            json!([0., 0., -0.01])
        );
        for change in [
            text.replace("time 2", "time 1"),
            text.replace("NALL", "TOP"),
            text.replace("-1D-6", "NaN"),
            text.replace("time 1", "time 0.1"),
            format!("{text}1 1 0 0 -1D3 0 0 0\n"),
        ] {
            assert!(parse_dat(&change).is_err());
        }
    }
}
