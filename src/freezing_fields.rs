//! Independent reconstruction of complete original Stefan fields and boundary energy.
use crate::{Result, contracts::invalid, freezing::FreezingReferenceSpec};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};

#[link(name = "m")]
unsafe extern "C" {
    fn erf(x: f64) -> f64;
}

fn error_function(x: f64) -> f64 {
    // libm's pure scalar erf has no pointer/state preconditions. The target is
    // the specified x86_64 Linux native runtime; all callers supply finite x.
    unsafe { erf(x) }
}

fn parameter(stefan: f64) -> f64 {
    let (mut lower, mut upper) = (0., 2.);
    for _ in 0..100 {
        let x: f64 = (lower + upper) / 2.;
        if x * (x * x).exp() * error_function(x) < stefan / std::f64::consts::PI.sqrt() {
            lower = x;
        } else {
            upper = x;
        }
    }
    (lower + upper) / 2.
}

fn number(value: &Value) -> Result<f64> {
    value
        .as_f64()
        .filter(|v| v.is_finite())
        .ok_or_else(|| invalid("finite native freezing quantity required"))
}
fn close(actual: f64, expected: f64, absolute: f64) -> bool {
    actual.is_finite() && (actual - expected).abs() <= absolute + 5e-12 * expected.abs()
}
fn scalar(text: &str) -> Result<f64> {
    text.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| invalid("finite native freezing CSV quantity required"))
}
fn sum(values: impl IntoIterator<Item = f64>) -> f64 {
    let (mut total, mut correction) = (0., 0.);
    for value in values {
        let increment = value - correction;
        let next = total + increment;
        correction = (next - total) - increment;
        total = next;
    }
    total
}

fn original(root: &Path, name: &str, hashes: &Value) -> Result<Vec<u8>> {
    let bytes =
        crate::worker::read_bounded(&crate::storage::safe_path(root, name)?, 32 * 1024 * 1024)?;
    if hashes[name] != format!("{:x}", Sha256::digest(&bytes)) {
        return Err(invalid(
            "original freezing bytes differ from verified receipt",
        ));
    }
    Ok(bytes)
}

fn text(bytes: &[u8]) -> Result<&str> {
    std::str::from_utf8(bytes).map_err(|_| invalid("text native freezing fields required"))
}

#[derive(Debug)]
struct Grid {
    nodes: Vec<[f64; 3]>,
    energy: f64,
    liquid_mass: f64,
    temperature_error: f64,
    front_error: f64,
}

fn grid(spec: &FreezingReferenceSpec, step: u64, bytes: &[u8]) -> Result<Grid> {
    let scale = spec.scale()?;
    let n = spec.resolution as usize;
    let span = spec.melting_temperature_k - spec.cold_wall_temperature_k;
    let initial_h = scale.initial_specific_enthalpy_j_kg;
    let p = parameter(scale.stefan_number);
    let elapsed = step as f64 / (6. * (n * n) as f64);
    let front = 2. * p * elapsed.sqrt();
    let mut lines = text(bytes)?.lines();
    if lines.next()
        != Some("i,j,x_m,y_m,material,specific_enthalpy_j_kg,temperature_k,liquid_fraction")
    {
        return Err(invalid("exact native freezing grid columns required"));
    }
    let mut nodes = vec![None; n * (n / 8)];
    let mut temperature_error: f64 = 0.;
    for line in lines {
        let columns = line.split(',').collect::<Vec<_>>();
        if columns.len() != 8 {
            return Err(invalid("complete native freezing CSV row required"));
        }
        let integer = |i: usize| {
            columns[i]
                .parse::<usize>()
                .map_err(|_| invalid("exact native integer grid identity required"))
        };
        let (i, j, m) = (integer(0)?, integer(1)?, integer(4)?);
        if i >= n || j >= n / 8 || m != if i == 0 { 3 } else { 1 } {
            return Err(invalid("original freezing grid/material scope changed"));
        }
        let (x, y, h, t, f) = (
            scalar(columns[2])?,
            scalar(columns[3])?,
            scalar(columns[5])?,
            scalar(columns[6])?,
            scalar(columns[7])?,
        );
        let theta = (t - spec.cold_wall_temperature_k) / span;
        if !close(x, i as f64 * scale.spacing_m, scale.spacing_m * 1e-15)
            || !close(y, (j as f64 + 0.5) * scale.spacing_m, 0.)
            || !(0. ..=1.).contains(&f)
            || !(spec.material_temperature_domain_k[0]..=spec.material_temperature_domain_k[1])
                .contains(&t)
            || !close(
                h,
                spec.specific_heat_j_kg_k * (t - spec.cold_wall_temperature_k)
                    + spec.latent_heat_j_kg * f,
                initial_h * 5e-12,
            )
            || (f - ((h - spec.specific_heat_j_kg_k * span) / spec.latent_heat_j_kg).clamp(0., 1.))
                .abs()
                > 5e-13
        {
            return Err(invalid(
                "native enthalpy/phase/temperature or SI coordinate relation changed",
            ));
        }
        if (i == 0 && (theta.abs() > 1e-12 || f != 0. || h.abs() > initial_h * 1e-12))
            || (i > 0
                && step == 0
                && ((theta - 1.).abs() > 1e-12 || f != 1. || (h / initial_h - 1.).abs() > 1e-12))
        {
            return Err(invalid(
                "fully liquid initial state or cold Dirichlet wall changed",
            ));
        }
        if step > 0 {
            let normalized_x = x / spec.size_m[0];
            let expected = if normalized_x < front {
                error_function(normalized_x / (2. * elapsed.sqrt())) / error_function(p)
            } else {
                1.
            };
            temperature_error = temperature_error.max((theta - expected).abs());
        }
        if nodes[j * n + i].replace([h, t, f]).is_some() {
            return Err(invalid("duplicate original freezing node"));
        }
    }
    let nodes = nodes
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| invalid("incomplete original freezing grid"))?;
    let active = || {
        nodes
            .iter()
            .enumerate()
            .filter(|(i, _)| i % n != 0)
            .map(|(_, v)| v)
    };
    let energy = sum(active().map(|v| v[0] * scale.cell_mass_kg));
    let liquid_mass = sum(active().map(|v| v[2] * scale.cell_mass_kg));
    let mut front_error: f64 = 0.;
    if step > 0 {
        for row in nodes.chunks_exact(n) {
            let intersections = row
                .windows(2)
                .enumerate()
                .filter(|(_, p)| p[0][2] <= 0.5 && p[1][2] > 0.5)
                .collect::<Vec<_>>();
            let [(i, pair)] = intersections.as_slice() else {
                return Err(invalid(
                    "one complete native solidification front per periodic row required",
                ));
            };
            let crossing = (*i as f64 + (0.5 - pair[0][2]) / (pair[1][2] - pair[0][2])) / n as f64;
            front_error = front_error.max((crossing - front).abs());
        }
    }
    Ok(Grid {
        nodes,
        energy,
        liquid_mass,
        temperature_error,
        front_error,
    })
}

fn vtk(spec: &FreezingReferenceSpec, bytes: &[u8], nodes: &[[f64; 3]]) -> Result<()> {
    let document = roxmltree::Document::parse(text(bytes)?)
        .map_err(|_| invalid("valid freezing VTK XML required"))?;
    let root = document.root_element();
    let n = spec.resolution as usize;
    let scale = spec.scale()?;
    let extent = format!("0 {} 0 {} 0 0", n - 1, n / 8 - 1);
    let image = root
        .children()
        .find(|v| v.has_tag_name("ImageData"))
        .ok_or_else(|| invalid("freezing image grid required"))?;
    let parse = |key| -> Result<Vec<f64>> {
        image
            .attribute(key)
            .ok_or_else(|| invalid("explicit VTK coordinates required"))?
            .split_whitespace()
            .map(scalar)
            .collect()
    };
    if !root.has_tag_name("VTKFile")
        || root.attribute("type") != Some("ImageData")
        || root.attribute("version") != Some("1.0")
        || root.attribute("byte_order") != Some("LittleEndian")
        || root.children().filter(|v| v.is_element()).count() != 1
        || image.children().filter(|v| v.is_element()).count() != 1
        || image.attribute("WholeExtent") != Some(extent.as_str())
        || parse("Origin")? != [0., scale.spacing_m / 2., 0.]
        || parse("Spacing")? != [scale.spacing_m, scale.spacing_m, spec.size_m[2]]
    {
        return Err(invalid("freezing VTK geometry/point association changed"));
    }
    let pieces = image
        .children()
        .filter(|v| v.has_tag_name("Piece"))
        .collect::<Vec<_>>();
    let [piece] = pieces.as_slice() else {
        return Err(invalid("one complete VTK piece required"));
    };
    if piece.attribute("Extent") != Some(extent.as_str()) {
        return Err(invalid("complete VTK extent required"));
    }
    let points = piece
        .children()
        .find(|v| v.has_tag_name("PointData"))
        .ok_or_else(|| invalid("native point association required"))?;
    let cells = piece.children().find(|v| v.has_tag_name("CellData"));
    if piece.children().filter(|v| v.is_element()).count() != 2
        || cells.is_none_or(|v| v.children().any(|v| v.is_element()))
    {
        return Err(invalid(
            "point-only freezing VTK with empty cell fields required",
        ));
    }
    let arrays = points
        .children()
        .filter(|v| v.is_element())
        .collect::<Vec<_>>();
    if arrays.len() != 6 {
        return Err(invalid("six complete native VTK arrays required"));
    }
    for (index, (name, kind, unit)) in [
        ("i", "Int32", "1"),
        ("j", "Int32", "1"),
        ("material", "Int32", "1"),
        ("specific_enthalpy_j_kg", "Float64", "J/kg"),
        ("temperature_k", "Float64", "K"),
        ("liquid_fraction", "Float64", "1"),
    ]
    .into_iter()
    .enumerate()
    {
        let array = arrays[index];
        if !array.has_tag_name("DataArray")
            || array.attribute("Name") != Some(name)
            || array.attribute("type") != Some(kind)
            || array.attribute("format") != Some("ascii")
            || array.attribute("NumberOfComponents") != Some("1")
            || array.attribute("unit") != Some(unit)
        {
            return Err(invalid(
                "original VTK field units, type or association changed",
            ));
        }
        let values = array
            .text()
            .unwrap_or("")
            .split_whitespace()
            .map(scalar)
            .collect::<Result<Vec<_>>>()?;
        if values.len() != nodes.len()
            || values.iter().enumerate().any(|(i, value)| {
                *value
                    != match index {
                        0 => (i % n) as f64,
                        1 => (i / n) as f64,
                        2 => {
                            if i % n == 0 {
                                3.
                            } else {
                                1.
                            }
                        }
                        _ => nodes[i][index - 3],
                    }
            })
        {
            return Err(invalid(
                "portable Float64 VTK values differ from complete original CSV grid",
            ));
        }
    }
    Ok(())
}

pub(crate) fn verify(
    spec: &FreezingReferenceSpec,
    root: &Path,
    value: &Value,
) -> Result<crate::qualification::NumericalEvidence> {
    let scale = spec.scale()?;
    if value["schema_version"] != 1
        || value["adapter"] != "OpenLB"
        || value["backend"] != "cpu"
        || value["precision"] != "float64"
        || value["executed"] != true
        || value["software_fallback"] != false
        || value["synthetic"] != true
        || value["dimensionality"] != 2
        || value["formulation"] != spec.formulation
        || value["source_revision"] != "145cd54810b468f4b6fd3ed86b10644264841578"
        || value["request"] != serde_json::to_value(spec)?
        || value["request_sha256"] != crate::contracts::digest(spec)?
        || value["physical_validation"] != "unqualified"
        || value["energy_zero"] != "solid at prescribed cold wall temperature"
        || value["moisture_risk"] != serde_json::to_value(&spec.moisture_risk)?
        || value["shape"] != serde_json::to_value(scale.shape)?
        || value["active_control_bounds_m"] != serde_json::to_value(scale.active_control_bounds_m)?
    {
        return Err(invalid(
            "exact source-bound native freezing recipe, geometry and execution identity required",
        ));
    }
    for (name, expected) in [
        ("spacing_m", scale.spacing_m),
        ("physical_step_s", scale.physical_step_s),
        ("stefan_number", scale.stefan_number),
        ("cell_mass_kg", scale.cell_mass_kg),
        ("active_volume_m3", scale.active_volume_m3),
    ] {
        if !close(number(&value[name])?, expected, 0.) {
            return Err(invalid("native freezing SI converter changed"));
        }
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
    if value["sandbox"]["policy"] != crate::freezing::SANDBOX_POLICY
        || value["sandbox"]["checks"]
            .as_object()
            .is_none_or(|v| v.len() != canaries.len())
        || canaries
            .iter()
            .any(|k| value["sandbox"]["checks"][k] != true)
    {
        return Err(invalid(
            "complete operation-specific native freezing sandbox required",
        ));
    }
    let hashes = &value["original_files_sha256"];
    if hashes
        .as_object()
        .is_none_or(|v| v.len() != 3 + 2 * spec.observation_steps.len())
    {
        return Err(invalid("exact original freezing/export hashes required"));
    }
    let native: Value =
        serde_json::from_slice(&original(root, "native-freezing-receipt.json", hashes)?)?;
    if native["numerical_verification"]
        != "independent complete-field Stefan/mass/energy gate required"
        || native.as_object().is_none_or(|v| {
            v.iter()
                .any(|(k, v)| k != "numerical_verification" && value[k] != *v)
        })
    {
        return Err(invalid(
            "native freezing metadata changed in verified receipt",
        ));
    }
    let ledger = original(root, "heat-exchange.csv", hashes)?;
    let mut lines = text(&ledger)?.lines();
    if lines.next() != Some("step,time_s,cold_exchange_j,reflecting_exchange_j") {
        return Err(invalid("exact native boundary exchange columns required"));
    }
    let mut exchanges = BTreeMap::from([(0, [0., 0.])]);
    let (mut cumulative, mut count) = ([0., 0.], 0u64);
    for line in lines {
        count += 1;
        let row = line.split(',').collect::<Vec<_>>();
        if row.len() != 4
            || row[0].parse::<u64>().ok() != Some(count)
            || count > spec.steps
            || !close(scalar(row[1])?, count as f64 * scale.physical_step_s, 0.)
        {
            return Err(invalid("complete ordered native energy schedule required"));
        }
        cumulative[0] += scalar(row[2])?;
        cumulative[1] += scalar(row[3])?;
        if spec.observation_steps.contains(&count) {
            exchanges.insert(count, cumulative);
        }
    }
    if count != spec.steps {
        return Err(invalid("boundary energy ledger truncated"));
    }
    let snapshots = value["snapshots"]
        .as_array()
        .filter(|v| v.len() == spec.observation_steps.len())
        .ok_or_else(|| invalid("complete freezing snapshots required"))?;
    let independent = value["independent_observations"]
        .as_array()
        .filter(|v| v.len() == snapshots.len())
        .ok_or_else(|| invalid("complete independent freezing observations required"))?;
    let mass = scale.cell_mass_kg * f64::from(spec.resolution - 1) * f64::from(spec.resolution / 8);
    let initial_energy = mass * scale.initial_specific_enthalpy_j_kg;
    let mut errors = [0f64; 4];
    for ((step, snapshot), observation) in spec
        .observation_steps
        .iter()
        .zip(snapshots)
        .zip(independent)
    {
        let file = format!("freezing-{step}.csv");
        if snapshot["step"] != *step
            || snapshot["path"] != file
            || observation["step"] != *step
            || !close(
                number(&snapshot["time_s"])?,
                *step as f64 * scale.physical_step_s,
                0.,
            )
            || !close(
                number(&observation["physical_time_s"])?,
                *step as f64 * scale.physical_step_s,
                0.,
            )
        {
            return Err(invalid("exact freezing path/step/physical time required"));
        }
        let fields = grid(spec, *step, &original(root, &file, hashes)?)?;
        vtk(
            spec,
            &original(root, &format!("freezing-{step}.vti"), hashes)?,
            &fields.nodes,
        )?;
        let exchange = exchanges
            .get(step)
            .ok_or_else(|| invalid("original boundary exchange at retained time required"))?;
        let balance =
            (fields.energy - initial_energy - exchange[0] - exchange[1]).abs() / initial_energy;
        errors[0] = errors[0].max(fields.front_error);
        errors[1] = errors[1].max(fields.temperature_error);
        errors[3] = errors[3]
            .max(balance)
            .max(exchange[1].abs() / initial_energy);
        for (key, expected) in [
            ("energy_j", fields.energy),
            ("mass_kg", mass),
            ("liquid_mass_kg", fields.liquid_mass),
            ("cold_exchange_j", exchange[0]),
            ("reflecting_exchange_j", exchange[1]),
        ] {
            let bound = if key.ends_with("_j") {
                initial_energy
            } else {
                mass
            };
            if !close(number(&snapshot[key])?, expected, 5e-12 * bound) {
                return Err(invalid("freezing summary differs from original field/flux"));
            }
        }
        for (key, expected) in [
            ("energy_j", fields.energy),
            ("mass_kg", mass),
            ("liquid_mass_kg", fields.liquid_mass),
            ("solid_mass_kg", mass - fields.liquid_mass),
            ("energy_relative_error", balance),
            (
                "reference_front_m",
                2. * parameter(scale.stefan_number)
                    * (*step as f64 / (6. * f64::from(spec.resolution).powi(2))).sqrt()
                    * spec.size_m[0],
            ),
        ] {
            if !close(
                number(&observation[key])?,
                expected,
                if key == "energy_relative_error" {
                    // Independent summation and SI multiplication order can
                    // differ by a few Float64 ulps of the initial energy.
                    5e-12
                } else {
                    5e-12
                        * expected.abs().max(if key.ends_with("_kg") {
                            mass
                        } else if key == "energy_j" {
                            initial_energy
                        } else {
                            1e-10
                        })
                },
            ) {
                return Err(invalid(
                    "independent native observation differs from original fields",
                ));
            }
        }
    }
    let pvd = original(root, "freezing.pvd", hashes)?;
    let document = roxmltree::Document::parse(text(&pvd)?)
        .map_err(|_| invalid("valid physical-time collection required"))?;
    let collection_root = document.root_element();
    let collection = collection_root
        .children()
        .find(|v| v.has_tag_name("Collection"));
    if !collection_root.has_tag_name("VTKFile")
        || collection_root.attribute("type") != Some("Collection")
        || collection_root.attribute("version") != Some("1.0")
        || collection_root.attribute("byte_order") != Some("LittleEndian")
        || collection_root
            .children()
            .filter(|v| v.is_element())
            .count()
            != 1
        || collection.is_none_or(|v| {
            v.children()
                .filter(|v| v.is_element())
                .any(|v| !v.has_tag_name("DataSet"))
        })
    {
        return Err(invalid(
            "one exact freezing physical-time VTK collection required",
        ));
    }
    let entries = document
        .descendants()
        .filter(|v| v.has_tag_name("DataSet"))
        .collect::<Vec<_>>();
    if entries.len() != snapshots.len()
        || entries
            .iter()
            .zip(&spec.observation_steps)
            .any(|(e, step)| {
                e.attribute("file") != Some(format!("freezing-{step}.vti").as_str())
                    || e.attribute("group") != Some("")
                    || e.attribute("part") != Some("0")
                    || e.attribute("timestep")
                        .and_then(|v| v.parse::<f64>().ok())
                        .is_none_or(|v| !close(v, *step as f64 * scale.physical_step_s, 0.))
            })
    {
        return Err(invalid("portable native field time collection changed"));
    }
    let mut utilization: f64 = 0.;
    if value["numerical_verification"]
        .as_object()
        .is_none_or(|v| v.len() != 4)
    {
        return Err(invalid("four unchanged freezing gates required"));
    }
    for ((name, tolerance), error) in [
        ("front", spec.front_tolerance),
        ("temperature", spec.temperature_tolerance),
        ("mass", spec.mass_tolerance),
        ("energy", spec.energy_tolerance),
    ]
    .into_iter()
    .zip(errors)
    {
        let check = &value["numerical_verification"][name];
        if error > tolerance
            || check["passed"] != true
            || number(&check["tolerance"])? != tolerance
            || !close(number(&check["error"])?, error, 5e-12)
        {
            return Err(invalid("unchanged native freezing acceptance gate failed"));
        }
        utilization = utilization.max(error / tolerance);
    }
    Ok(crate::qualification::NumericalEvidence{reference:"independent complete original Float64 grids; analytical Stefan similarity temperature/front and actual population-boundary energy exchange".into(),scope:"synthetic equal-phase conduction solidification in explicit nodal control volumes; no expansion, pressure, retained-water transfer or physical qualification".into(),error_kind:"maximum_approved_gate_fraction".into(),error:utilization,tolerance:1.})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture() -> (tempfile::TempDir, FreezingReferenceSpec, Value) {
        let mut spec: FreezingReferenceSpec =
            serde_json::from_str(include_str!("../examples/freezing-reference.json")).unwrap();
        spec.resolution = 32;
        spec.steps = 1024;
        spec.observation_steps = vec![0, 1024];
        let root = tempfile::tempdir().unwrap();
        let dx = 0.001 / 32.;
        let dt = 1. / 6144.;
        let mass_cell = 1000. * dx * dx * 0.001;
        let mass = mass_cell * 31. * 4.;
        let initial_energy = 110000. * mass;
        // Independent published similarity root at Ste=0.1, not the verifier's
        // bisection; sharp manufactured phase at its exact similarity front.
        let p = 0.22001627274293786;
        let front = 2. * p * (1f64 / 6.).sqrt();
        let mut snapshots = Vec::new();
        let mut observations = Vec::new();
        let mut final_exchange = 0.;
        for step in [0, 1024] {
            let mut csv =
                "i,j,x_m,y_m,material,specific_enthalpy_j_kg,temperature_k,liquid_fraction\n"
                    .to_string();
            let mut arrays = vec![Vec::<f64>::new(); 6];
            let (mut energy, mut liquid) = (0., 0.);
            for j in 0..4 {
                for i in 0..32 {
                    let x = i as f64 / 32.;
                    let theta = if i == 0 {
                        0.
                    } else if step == 0 || x >= front {
                        1.
                    } else {
                        error_function(x / (2. * (1f64 / 6.).sqrt())) / error_function(p)
                    };
                    let phase = if i != 0 && (step == 0 || x >= front) {
                        1.
                    } else {
                        0.
                    };
                    let h = 10000. * theta + 100000. * phase;
                    let temperature = 263.15 + 10. * theta;
                    csv.push_str(&format!(
                        "{i},{j},{:.17e},{:.17e},{},{h:.17e},{temperature:.17e},{phase:.17e}\n",
                        i as f64 * dx,
                        (j as f64 + 0.5) * dx,
                        if i == 0 { 3 } else { 1 }
                    ));
                    let values = [
                        i as f64,
                        j as f64,
                        if i == 0 { 3. } else { 1. },
                        h,
                        temperature,
                        phase,
                    ];
                    for (array, value) in arrays.iter_mut().zip(values) {
                        array.push(value);
                    }
                    if i != 0 {
                        energy += mass_cell * h;
                        liquid += mass_cell * phase;
                    }
                }
            }
            let name = format!("freezing-{step}.csv");
            fs::write(root.path().join(&name), csv).unwrap();
            let exchange = if step == 0 {
                0.
            } else {
                energy - initial_energy
            };
            if step != 0 {
                final_exchange = exchange;
            }
            snapshots.push(serde_json::json!({"path":name,"step":step,"time_s":step as f64*dt,"energy_j":energy,"mass_kg":mass,"liquid_mass_kg":liquid,"cold_exchange_j":exchange,"reflecting_exchange_j":0.}));
            observations.push(serde_json::json!({"step":step,"physical_time_s":step as f64*dt,"reference_front_m":if step==0 {0.} else {front*0.001},"mass_kg":mass,"liquid_mass_kg":liquid,"solid_mass_kg":mass-liquid,"energy_j":energy,"energy_relative_error":0.}));
            let mut xml = format!(
                "<VTKFile type=\"ImageData\" version=\"1.0\" byte_order=\"LittleEndian\"><ImageData WholeExtent=\"0 31 0 3 0 0\" Origin=\"0 {:.17e} 0\" Spacing=\"{dx:.17e} {dx:.17e} 0.001\"><Piece Extent=\"0 31 0 3 0 0\"><PointData>",
                dx / 2.
            );
            for ((name, kind, unit), values) in [
                ("i", "Int32", "1"),
                ("j", "Int32", "1"),
                ("material", "Int32", "1"),
                ("specific_enthalpy_j_kg", "Float64", "J/kg"),
                ("temperature_k", "Float64", "K"),
                ("liquid_fraction", "Float64", "1"),
            ]
            .into_iter()
            .zip(arrays)
            {
                xml.push_str(&format!("<DataArray type=\"{kind}\" Name=\"{name}\" unit=\"{unit}\" NumberOfComponents=\"1\" format=\"ascii\">{}</DataArray>",values.into_iter().map(|v|format!("{v:.17e}")).collect::<Vec<_>>().join(" ")));
            }
            xml.push_str("</PointData><CellData/></Piece></ImageData></VTKFile>");
            fs::write(root.path().join(format!("freezing-{step}.vti")), xml).unwrap();
        }
        let mut ledger = "step,time_s,cold_exchange_j,reflecting_exchange_j\n".to_string();
        for step in 1..=1024 {
            ledger.push_str(&format!(
                "{step},{:.17e},{:.17e},0\n",
                step as f64 * dt,
                if step == 1024 { final_exchange } else { 0. }
            ));
        }
        fs::write(root.path().join("heat-exchange.csv"), ledger).unwrap();
        fs::write(root.path().join("freezing.pvd"),format!("<VTKFile type=\"Collection\" version=\"1.0\" byte_order=\"LittleEndian\"><Collection><DataSet file=\"freezing-0.vti\" part=\"0\" group=\"\" timestep=\"0\"/><DataSet file=\"freezing-1024.vti\" part=\"0\" group=\"\" timestep=\"{:.17e}\"/></Collection></VTKFile>",1024.*dt)).unwrap();
        let native = serde_json::json!({"schema_version":1,"adapter":"OpenLB","backend":"cpu","executed":true,"software_fallback":false,"precision":"float64","source_revision":"145cd54810b468f4b6fd3ed86b10644264841578","formulation":spec.formulation,"dimensionality":2,"synthetic":true,"request":spec,"shape":[33,4],"spacing_m":dx,"physical_step_s":dt,"stefan_number":0.1,"cell_mass_kg":mass_cell,"active_volume_m3":31.*4.*dx*dx*0.001,"active_control_bounds_m":[[dx/2.,0.001-dx/2.],[0.,0.000125],[0.,0.001]],"snapshots":snapshots,"energy_zero":"solid at prescribed cold wall temperature","physical_validation":"unqualified","numerical_verification":"independent complete-field Stefan/mass/energy gate required"});
        fs::write(
            root.path().join("native-freezing-receipt.json"),
            serde_json::to_vec(&native).unwrap(),
        )
        .unwrap();
        let mut receipt = native;
        receipt["request_sha256"] = crate::contracts::digest(&spec).unwrap().into();
        receipt["moisture_risk"] = serde_json::to_value(&spec.moisture_risk).unwrap();
        receipt["independent_observations"] = observations.into();
        receipt["numerical_verification"] = serde_json::json!({"front":{"error":(5.5/32.-front).abs(),"tolerance":0.02,"passed":true},"temperature":{"error":0.,"tolerance":0.02,"passed":true},"mass":{"error":0.,"tolerance":1e-10,"passed":true},"energy":{"error":0.,"tolerance":1e-10,"passed":true}});
        let checks = serde_json::Map::from_iter(
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
            .map(|k| (k.to_string(), true.into())),
        );
        receipt["sandbox"] =
            serde_json::json!({"policy":crate::freezing::SANDBOX_POLICY,"checks":checks});
        let mut hashes = serde_json::Map::new();
        for entry in fs::read_dir(root.path()).unwrap() {
            let entry = entry.unwrap();
            hashes.insert(
                entry.file_name().into_string().unwrap(),
                format!("{:x}", Sha256::digest(fs::read(entry.path()).unwrap())).into(),
            );
        }
        receipt["original_files_sha256"] = hashes.into();
        (root, spec, receipt)
    }

    #[test]
    fn complete_manufactured_similarity_fields_require_original_phase_enthalpy_flux_and_float64_exports()
     {
        let (root, spec, receipt) = fixture();
        let evidence = verify(&spec, root.path(), &receipt).unwrap();
        assert!(evidence.error < 1. && evidence.scope.contains("no expansion"));
        for key in [
            "request_sha256",
            "sandbox",
            "shape",
            "energy_zero",
            "independent_observations",
            "original_files_sha256",
            "source_revision",
            "physical_step_s",
            "active_control_bounds_m",
        ] {
            let mut changed = receipt.clone();
            changed[key] = Value::Null;
            assert!(verify(&spec, root.path(), &changed).is_err(), "{key}");
        }
        for key in ["front", "temperature", "mass", "energy"] {
            let mut changed = receipt.clone();
            changed["numerical_verification"][key]["tolerance"] = 1.0.into();
            assert!(verify(&spec, root.path(), &changed).is_err(), "{key}");
        }
        for (name, replace) in [
            ("freezing-1024.vti", ("unit=\"K\"", "unit=\"degC\"")),
            ("freezing.pvd", ("freezing-1024.vti", "../../foreign.vti")),
            ("freezing-1024.csv", ("31,3,", "31,2,")),
            ("heat-exchange.csv", ("1024,", "1025,")),
            (
                "freezing-0.csv",
                ("1.10000000000000000e5", "1.20000000000000000e5"),
            ),
        ] {
            let path = root.path().join(name);
            let before = fs::read_to_string(&path).unwrap();
            let after = before.replace(replace.0, replace.1);
            assert_ne!(after, before, "{name}");
            fs::write(&path, &after).unwrap();
            let mut changed = receipt.clone();
            changed["original_files_sha256"][name] =
                format!("{:x}", Sha256::digest(after.as_bytes())).into();
            assert!(
                verify(&spec, root.path(), &changed).is_err(),
                "self-consistent hash substitution: {name}"
            );
            fs::write(path, before).unwrap();
        }
    }

    #[test]
    fn freezing_registration_preserves_both_formats_and_rejects_missing_duplicate_or_changed_originals()
     {
        let (root, spec, receipt) = fixture();
        let plan =
            crate::contracts::ExecutionPlan::freezing_reference(spec, "research".into()).unwrap();
        let mut records = receipt["original_files_sha256"]
            .as_object()
            .unwrap()
            .keys()
            .map(|name| {
                let mut record =
                    crate::storage::native_manifest(root.path(), name, 32 * 1024 * 1024, "fixture")
                        .unwrap();
                record.path = format!("stages/freezing/{name}");
                record
            })
            .collect::<Vec<_>>();
        crate::freezing::annotate_fields(&plan, &mut records).unwrap();
        for record in records.iter().filter(|r| {
            r.path.ends_with(".vti")
                || (r.path.ends_with(".csv") && !r.path.ends_with("heat-exchange.csv"))
        }) {
            assert!(record.time_s.is_some());
            assert_eq!(record.association.as_deref(), Some("native_lattice_point"));
            assert!(record.units.as_ref().unwrap().contains("temperature_k:K"));
        }
        let missing = records
            .iter()
            .position(|r| r.path.ends_with("freezing-1024.vti"))
            .unwrap();
        let mut changed = records.clone();
        changed.remove(missing);
        assert!(crate::freezing::annotate_fields(&plan, &mut changed).is_err());
        records.push(records[missing].clone());
        assert!(crate::freezing::annotate_fields(&plan, &mut records).is_err());
    }
}
