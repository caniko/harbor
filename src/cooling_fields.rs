//! Reconstruct authoritative retained-water controls, enthalpy and boundary energy.
use crate::{
    Result,
    contracts::{digest, invalid},
    cooling_execution::CoolingExecutionSpec,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};

pub const FIELD_UNITS: &str = "i:1,j:1,parent_i:1,parent_j:1,x_m:m,y_m:m,water_fraction:1,specific_enthalpy_j_kg:J/kg,temperature_k:K,liquid_fraction:1";
pub const ASSOCIATION: &str = "native_original_parent_congruent_control";
const COLUMNS: &str = "i,j,parent_i,parent_j,x_m,y_m,water_fraction,specific_enthalpy_j_kg,temperature_k,liquid_fraction";

fn text(raw: &[u8]) -> Result<&str> {
    std::str::from_utf8(raw).map_err(|_| invalid("original cooling UTF-8 CSV required"))
}
fn scalar(raw: &str) -> Result<f64> {
    raw.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| invalid("finite original cooling scalar required"))
}
fn number(v: &Value) -> Result<f64> {
    v.as_f64()
        .filter(|v| v.is_finite())
        .ok_or_else(|| invalid("finite native cooling receipt quantity required"))
}
fn close(a: f64, b: f64, scale: f64) -> bool {
    a.is_finite() && b.is_finite() && (a - b).abs() <= 5e-12 * scale.abs()
}
#[derive(Default)]
struct Sum {
    total: f64,
    correction: f64,
}
impl Sum {
    fn add(&mut self, v: f64) {
        let increment = v - self.correction;
        let next = self.total + increment;
        self.correction = (next - self.total) - increment;
        self.total = next;
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Node {
    pub i: usize,
    pub j: usize,
    pub parent: [usize; 2],
    pub xy: [f64; 2],
    pub water_fraction: f64,
    pub enthalpy: f64,
    pub temperature: f64,
    pub liquid_fraction: f64,
}

fn parents(spec: &CoolingExecutionSpec, raw: &[u8]) -> Result<BTreeMap<[usize; 2], [f64; 3]>> {
    let initialization = crate::retained_cooling::reconstruct(
        &spec.source_wetting,
        &spec.request.initialization,
        raw,
    )?;
    if digest(&initialization)? != digest(&spec.prepared.initialization)? {
        return Err(invalid(
            "complete original retained cooling initialization changed",
        ));
    }
    let [nx, ny] = spec.prepared.retained.extrusion.source_grid_shape;
    let dx = spec.prepared.retained.extrusion.spacing_m;
    let mut lines = text(raw)?.lines();
    if lines.next() != Some("x_m,y_m,material,phi,u_lattice,v_lattice") {
        return Err(invalid("original native wetting columns required"));
    }
    let mut controls = BTreeMap::new();
    for line in lines {
        let row = line.split(',').collect::<Vec<_>>();
        if row.len() != 6 {
            return Err(invalid("complete wetting source rows required"));
        }
        if row[2] != "1" {
            continue;
        }
        let x = scalar(row[0])?;
        let y = scalar(row[1])?;
        let i = (x / dx).round() as usize;
        let j = (y / dx).round() as usize;
        let f = 1. - scalar(row[3])?;
        if i >= nx
            || j == 0
            || j >= ny - 1
            || !(0. ..=1.).contains(&f)
            || scalar(row[4])? != 0.
            || scalar(row[5])? != 0.
            || controls.insert([i, j], [x, y, f]).is_some()
        {
            return Err(invalid(
                "unchanged unique bounded stationary water controls required",
            ));
        }
    }
    if controls.len() != nx * (ny - 2) {
        return Err(invalid("every original water control required"));
    }
    Ok(controls)
}

pub(crate) fn nodes(
    spec: &CoolingExecutionSpec,
    original: &[u8],
    step: u64,
    raw: &[u8],
) -> Result<Vec<Node>> {
    spec.validate()?;
    if !spec.request.observation_steps().contains(&step) {
        return Err(invalid(
            "exact approved native cooling observation required",
        ));
    }
    let controls = parents(spec, original)?;
    let q = spec.request.spatial_refinement as usize;
    let [nx, ny] = spec.prepared.retained.extrusion.source_grid_shape;
    let nx = nx * q;
    let height = (ny - 2) * q;
    let dx = spec.prepared.retained.extrusion.spacing_m;
    let origin = spec.request.initialization.retained.destination_origin_m;
    let material = &spec.prepared.normalized;
    let cold = material.cold_wall_temperature_k;
    let tm = material.melting_temperature_k;
    let scale = material.specific_heat_j_kg_k * (tm - cold) + material.latent_heat_j_kg;
    let tolerance = spec
        .request
        .initialization
        .thermal
        .maximum_relative_conservation_error;
    let mut lines = text(raw)?.lines();
    if lines.next() != Some(COLUMNS) {
        return Err(invalid(
            "exact authoritative native cooling columns required",
        ));
    }
    let mut grid = vec![None; nx * height];
    for line in lines {
        let row = line.split(',').collect::<Vec<_>>();
        if row.len() != 10 {
            return Err(invalid("complete native cooling control row required"));
        }
        let integer = |index: usize| {
            row[index]
                .parse::<usize>()
                .map_err(|_| invalid("exact integer native cooling identity required"))
        };
        let (i, j) = (integer(0)?, integer(1)?);
        if i >= nx || j == 0 || j > height {
            return Err(invalid("native cooling subcontrol outside original domain"));
        }
        let parent = [integer(2)?, integer(3)?];
        if parent != [i / q, (j - 1) / q + 1] {
            return Err(invalid("exact congruent original cooling parent required"));
        }
        let [px, py, f] = controls[&parent];
        let xy = [scalar(row[4])?, scalar(row[5])?];
        let expected = [
            origin[0] + px + (((i % q) as f64 + 0.5) / q as f64 - 0.5) * dx,
            origin[1] + py + ((((j - 1) % q) as f64 + 0.5) / q as f64 - 0.5) * dx,
        ];
        let water_fraction = scalar(row[6])?;
        let enthalpy = scalar(row[7])?;
        let temperature = scalar(row[8])?;
        let liquid_fraction = scalar(row[9])?;
        let wanted = material.specific_heat_j_kg_k * (temperature - cold)
            + f * material.latent_heat_j_kg * liquid_fraction;
        if xy != expected
            || water_fraction != f
            || !(0. ..=1.).contains(&liquid_fraction)
            || temperature < cold - 1e-10
            || temperature > tm + 1e-10
            || (enthalpy - wanted).abs() / scale > tolerance
            || (step == 0 && (temperature != tm || (f > 0. && liquid_fraction != 1.)))
        {
            return Err(invalid(
                "unclipped original parent phase, SI subcontrols, initial state or native enthalpy relation changed",
            ));
        }
        let node = Node {
            i,
            j,
            parent,
            xy,
            water_fraction,
            enthalpy,
            temperature,
            liquid_fraction,
        };
        if grid[(j - 1) * nx + i].replace(node).is_some() {
            return Err(invalid("duplicate original cooling subcontrol"));
        }
    }
    grid.into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| invalid("complete original native cooling subcontrols required"))
}

fn original(root: &Path, name: &str, hashes: &Value, limit: u64) -> Result<Vec<u8>> {
    let raw = crate::worker::read_bounded(&crate::storage::safe_path(root, name)?, limit)?;
    if hashes[name] != format!("{:x}", Sha256::digest(&raw)) {
        return Err(invalid(
            "original native cooling bytes differ from verified receipt",
        ));
    }
    Ok(raw)
}

pub(crate) fn verify(
    spec: &CoolingExecutionSpec,
    source: &[u8],
    root: &Path,
    value: &Value,
) -> Result<crate::qualification::NumericalEvidence> {
    spec.validate()?;
    let envelope = spec.native_request()?;
    let native_request = &envelope["native_request"];
    let request = &spec.request;
    let material = &spec.prepared.normalized;
    let q = f64::from(request.spatial_refinement);
    let spacing = spec.prepared.retained.extrusion.spacing_m / q;
    let dt = spec.prepared.initialization.thermal_physical_step_s
        / (q * q * f64::from(request.integration_substeps));
    let mass =
        material.density_kg_m3 * spacing * spacing * spec.prepared.retained.extrusion.extrusion_m;
    let [nx, ny] = spec.prepared.retained.extrusion.source_grid_shape;
    let source_hash = format!("{:x}", Sha256::digest(source));
    for (key, expected) in [
        ("schema_version", serde_json::json!(1)),
        ("adapter", serde_json::json!("OpenLB")),
        ("backend", serde_json::json!("cpu")),
        ("precision", serde_json::json!("float64")),
        (
            "source_revision",
            serde_json::json!("145cd54810b468f4b6fd3ed86b10644264841578"),
        ),
        ("collision", serde_json::json!("native_total_enthalpy_trt")),
        ("trt_magic", serde_json::json!(0.25)),
        ("executed", serde_json::json!(true)),
        ("software_fallback", serde_json::json!(false)),
        ("request", native_request.clone()),
        ("request_sha256", serde_json::json!(digest(&envelope)?)),
        ("source_shape", serde_json::json!([nx, ny])),
        (
            "native_shape",
            serde_json::json!([
                nx * request.spatial_refinement as usize,
                (ny - 2) * request.spatial_refinement as usize + 2
            ]),
        ),
        (
            "boundary",
            serde_json::json!(
                "native_half_link_cold_ymin; native_half_link_insulated_ymax; periodic_x"
            ),
        ),
        ("physical_validation", serde_json::json!("unqualified")),
        ("original_source_sha256", serde_json::json!(source_hash)),
        (
            "native_driver_sha256",
            serde_json::json!(format!(
                "{:x}",
                Sha256::digest(include_bytes!("../adapters/openlb_retained_cooling.cpp"))
            )),
        ),
        (
            "field_units",
            serde_json::json!(
                "position:m,water_fraction:1,specific_enthalpy:J/kg,temperature:K,liquid_fraction:1"
            ),
        ),
        ("field_association", serde_json::json!(ASSOCIATION)),
        ("boundary_exchange_units", serde_json::json!("J")),
    ] {
        if value[key] != expected {
            return Err(invalid(format!(
                "exact source-bound native cooling identity {key} changed"
            )));
        }
    }
    for (key, expected) in [
        (
            "source_spacing_m",
            spec.prepared.retained.extrusion.spacing_m,
        ),
        ("spacing_m", spacing),
        ("physical_step_s", dt),
        ("cell_mass_kg", mass),
    ] {
        if !close(number(&value[key])?, expected, expected / 10.) {
            return Err(invalid("native cooling SI converter changed"));
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
        "original_source_readonly",
    ];
    if value["sandbox"]["policy"] != crate::cooling_execution::SANDBOX_POLICY
        || value["sandbox"]["checks"]
            .as_object()
            .is_none_or(|v| v.len() != canaries.len())
        || canaries
            .iter()
            .any(|k| value["sandbox"]["checks"][k] != true)
    {
        return Err(invalid(
            "complete source-readonly operation-only cooling sandbox required",
        ));
    }
    let raw_native = crate::worker::read_bounded(
        &root.join("native-retained-cooling-receipt.json"),
        256 * 1024,
    )?;
    let native: Value = serde_json::from_slice(&raw_native)?;
    if value["native_receipt_sha256"] != format!("{:x}", Sha256::digest(&raw_native))
        || native
            .as_object()
            .is_none_or(|o| o.iter().any(|(k, v)| value[k] != *v))
    {
        return Err(invalid(
            "original native cooling receipt differs from published reconstruction",
        ));
    }
    let independent = &value["independent_verification"];
    let hashes = &independent["original_files_sha256"];
    if hashes
        .as_object()
        .is_none_or(|v| v.len() != 1 + request.observation_base_steps.len())
        || independent["executed"] != false
        || independent["original_source_sha256"] != source_hash
    {
        return Err(invalid(
            "complete independent original cooling file identities required",
        ));
    }
    let ledger = original(root, "heat-exchange.csv", hashes, 128 * 1024 * 1024)?;
    let mut lines = text(&ledger)?.lines();
    if lines.next() != Some("step,time_s,cold_exchange_j,reflecting_exchange_j") {
        return Err(invalid(
            "original native cooling boundary-energy columns required",
        ));
    }
    let mut cold = Sum::default();
    let mut reflecting = Sum::default();
    let mut count = 0;
    let mut exchanges = BTreeMap::from([(0, [0., 0.])]);
    let steps = request.observation_steps();
    for line in lines {
        count += 1;
        let row = line.split(',').collect::<Vec<_>>();
        if row.len() != 4
            || row[0].parse::<u64>().ok() != Some(count)
            || count > request.native_steps()
            || !close(scalar(row[1])?, count as f64 * dt, count as f64 * dt / 10.)
        {
            return Err(invalid(
                "complete ordered native cooling boundary-energy history required",
            ));
        }
        cold.add(scalar(row[2])?);
        reflecting.add(scalar(row[3])?);
        if steps.contains(&count) {
            exchanges.insert(count, [cold.total, reflecting.total]);
        }
    }
    if count != request.native_steps() {
        return Err(invalid("original cooling energy history truncated"));
    }
    let snapshots = value["snapshots"]
        .as_array()
        .filter(|v| v.len() == steps.len())
        .ok_or_else(|| invalid("complete original cooling observations required"))?;
    let observations = independent["observations"]
        .as_array()
        .filter(|v| v.len() == steps.len())
        .ok_or_else(|| invalid("complete reconstructed cooling observations required"))?;
    let initial = spec.prepared.initialization.initial_total_energy_j;
    let water = spec.prepared.initialization.conserved_water_mass_kg;
    let hscale = material.specific_heat_j_kg_k
        * (material.melting_temperature_k - material.cold_wall_temperature_k)
        + material.latent_heat_j_kg;
    let mut errors = [0f64; 5];
    for ((step, snapshot), observation) in steps.iter().zip(snapshots).zip(observations) {
        let path = format!("cooling-{step}.csv");
        if snapshot["path"] != path
            || snapshot["step"] != *step
            || observation["step"] != *step
            || !close(
                number(&snapshot["time_s"])?,
                *step as f64 * dt,
                *step as f64 * dt / 10.,
            )
            || !close(
                number(&observation["physical_time_s"])?,
                *step as f64 * dt,
                *step as f64 * dt / 10.,
            )
        {
            return Err(invalid(
                "exact original cooling step and physical time required",
            ));
        }
        let grid = nodes(
            spec,
            source,
            *step,
            &original(root, &path, hashes, 64 * 1024 * 1024)?,
        )?;
        let mut energy = Sum::default();
        let mut measured_water = Sum::default();
        let mut liquid = Sum::default();
        let mut minimum = f64::INFINITY;
        let mut maximum = f64::NEG_INFINITY;
        for node in grid {
            energy.add(mass * node.enthalpy);
            measured_water.add(mass * node.water_fraction);
            liquid.add(mass * node.water_fraction * node.liquid_fraction);
            minimum = minimum.min(node.temperature);
            maximum = maximum.max(node.temperature);
            errors[4] = errors[4].max(
                (node.enthalpy
                    - material.specific_heat_j_kg_k
                        * (node.temperature - material.cold_wall_temperature_k)
                    - node.water_fraction * material.latent_heat_j_kg * node.liquid_fraction)
                    .abs()
                    / hscale,
            );
        }
        let exchange = exchanges[step];
        errors[0] = errors[0].max((measured_water.total / water - 1.).abs());
        if *step == 0 {
            errors[1] = (energy.total / initial - 1.).abs();
        }
        errors[2] =
            errors[2].max((energy.total - initial - exchange[0] - exchange[1]).abs() / initial);
        errors[3] = errors[3].max(exchange[1].abs() / initial);
        for (key, expected, scale) in [
            ("energy_j", energy.total, initial),
            ("water_mass_kg", measured_water.total, water),
            ("liquid_water_mass_kg", liquid.total, water),
            ("cold_exchange_j", exchange[0], initial),
            ("reflecting_exchange_j", exchange[1], initial),
        ] {
            if !close(number(&snapshot[key])?, expected, scale) {
                return Err(invalid(
                    "native cooling summary differs from complete original controls and boundary exchange",
                ));
            }
            if !key.ends_with("exchange_j") && !close(number(&observation[key])?, expected, scale) {
                return Err(invalid(
                    "published cooling observation differs from original reconstruction",
                ));
            }
        }
        for (key, expected, scale) in [
            (
                "solid_water_mass_kg",
                measured_water.total - liquid.total,
                water,
            ),
            ("minimum_temperature_k", minimum, 1.),
            ("maximum_temperature_k", maximum, 1.),
        ] {
            if !close(number(&observation[key])?, expected, scale) {
                return Err(invalid(
                    "published cooling extrema or solid mass differs from original controls",
                ));
            }
        }
    }
    let tolerance = request
        .initialization
        .thermal
        .maximum_relative_conservation_error;
    let names = [
        "water_mass_relative_error",
        "initial_enthalpy_relative_error",
        "energy_balance_relative_error",
        "reflecting_exchange_relative_error",
        "phase_enthalpy_normalized_error",
    ];
    if independent["checks"]
        .as_object()
        .is_none_or(|v| v.len() != names.len())
    {
        return Err(invalid("all separate cooling conservation gates required"));
    }
    for (name, error) in names.into_iter().zip(errors) {
        let check = &independent["checks"][name];
        if !error.is_finite()
            || error > tolerance
            || check["passed"] != true
            || number(&check["tolerance"])? != tolerance
            || number(&check["error"])? < 0.
            || number(&check["error"])? > tolerance
            || !close(number(&check["error"])?, error, 1.)
        {
            return Err(invalid(
                "unchanged independently reconstructed cooling conservation gate failed",
            ));
        }
    }
    Ok(crate::qualification::NumericalEvidence {reference:"complete original parent/subcontrol phase, mass, enthalpy and native half-link boundary-energy ledger".into(),scope:"stationary synthetic equal-property retained-water conduction; native execution, uniform analytic/spatial/temporal convergence and physical validation separate".into(),error_kind:"maximum_relative_conservation_error".into(),error:errors.into_iter().fold(0.,f64::max),tolerance})
}

pub(crate) fn annotate(
    plan: &crate::contracts::ExecutionPlan,
    artifacts: &mut [crate::contracts::ArtifactManifest],
) -> Result<()> {
    let Some(spec) = &plan.retained_cooling else {
        return Ok(());
    };
    for (step, time) in spec
        .request
        .observation_steps()
        .into_iter()
        .zip(spec.times_s())
    {
        let path = format!("stages/retained-cooling/cooling-{step}.csv");
        let fields = artifacts
            .iter_mut()
            .filter(|r| r.path == path)
            .collect::<Vec<_>>();
        let [field] = fields.as_slice() else {
            return Err(invalid(
                "every approved original cooling field must be registered exactly once",
            ));
        };
        if field.format != "csv" || field.bytes == 0 || field.bytes > 64 * 1024 * 1024 {
            return Err(invalid("bounded original cooling CSV required"));
        }
        for field in fields {
            field.time_s = Some(time);
            field.association = Some(ASSOCIATION.into());
            field.units = Some(FIELD_UNITS.into());
            field.provenance="original native Float64 retained-water enthalpy/control fields; no clipped phase, extrapolated time or physical qualification".into();
        }
    }
    Ok(())
}

pub(crate) fn registered(
    store: &crate::storage::Store,
    id: &str,
    plan: &crate::contracts::ExecutionPlan,
    value: &Value,
) -> Result<crate::qualification::NumericalEvidence> {
    let spec = plan
        .retained_cooling
        .as_ref()
        .ok_or_else(|| invalid("source-bound cooling recipe required"))?;
    let original = crate::cooling_execution::registered(store, id, plan)?;
    let source = crate::worker::read_bounded(&original, 16 * 1024 * 1024)?;
    let root = store.job_dir(id)?;
    let mut files = value["independent_verification"]["original_files_sha256"]
        .as_object()
        .ok_or_else(|| invalid("complete cooling originals required"))?
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    files.push("native-retained-cooling-receipt.json".into());
    for file in files {
        let path = format!("stages/retained-cooling/{file}");
        let record = store
            .artifact_record(id, &path)?
            .ok_or_else(|| invalid("registered original cooling file missing"))?;
        let observed = crate::storage::native_manifest(
            &root,
            &path,
            128 * 1024 * 1024,
            "verify complete original cooling bytes",
        )?;
        if record.sha256 != observed.sha256 || record.bytes != observed.bytes || record.bytes == 0 {
            return Err(invalid("registered original cooling file bytes changed"));
        }
        if let Some(index) = spec
            .request
            .observation_steps()
            .iter()
            .position(|s| file == format!("cooling-{s}.csv"))
            && (record.time_s != Some(spec.times_s()[index])
                || record.association.as_deref() != Some(ASSOCIATION)
                || record.units.as_deref() != Some(FIELD_UNITS)
                || record.format != "csv")
        {
            return Err(invalid(
                "original cooling field units, time or association changed",
            ));
        }
    }
    let (_, request) =
        crate::results::registered(store, id, "native-retained-cooling-request.json")?;
    if request != spec.native_request()? {
        return Err(invalid(
            "registered original cooling native approval envelope changed",
        ));
    }
    verify(spec, &source, &root.join("stages/retained-cooling"), value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, CoolingExecutionSpec, Vec<u8>, Value) {
        let (_, mut spec, source) = crate::cooling_execution_tests::fixture();
        spec.request.spatial_refinement = 1;
        let root = tempfile::tempdir().unwrap();
        let thermal = &spec.prepared.normalized;
        let cold = thermal.cold_wall_temperature_k;
        let tm = thermal.melting_temperature_k;
        let cp = thermal.specific_heat_j_kg_k;
        let latent = thermal.latent_heat_j_kg;
        let dt = spec.prepared.initialization.thermal_physical_step_s;
        let dx = spec.prepared.retained.extrusion.spacing_m;
        let mass = thermal.density_kg_m3 * dx * dx * spec.prepared.retained.extrusion.extrusion_m;
        let controls = parents(&spec, &source).unwrap();
        let final_energy = controls.len() as f64 * mass * cp * (tm - cold - 1.);
        let initial = spec.prepared.initialization.initial_total_energy_j;
        let delta = final_energy - initial;
        let mut ledger = "step,time_s,cold_exchange_j,reflecting_exchange_j\n".to_string();
        for step in 1..=spec.request.native_steps() {
            ledger.push_str(&format!(
                "{step},{:.17e},{:.17e},0\n",
                step as f64 * dt,
                if step == 1 { delta } else { 0. }
            ));
        }
        fs_write(root.path(), "heat-exchange.csv", ledger.as_bytes());
        let mut hashes = serde_json::Map::from_iter([(
            "heat-exchange.csv".into(),
            serde_json::json!(format!("{:x}", Sha256::digest(ledger.as_bytes()))),
        )]);
        let mut snapshots = vec![];
        let mut observations = vec![];
        for step in spec.request.observation_steps() {
            let mut raw = format!("{COLUMNS}\n");
            let mut energy = 0.;
            let mut water = 0.;
            let mut liquid = 0.;
            let t = if step == 0 { tm } else { tm - 1. };
            for ([i, j], [x, y, f]) in &controls {
                let phase = if step == 0 && *f > 0. { 1. } else { 0. };
                let h = cp * (t - cold) + f * latent * phase;
                let xx = spec.request.initialization.retained.destination_origin_m[0] + x;
                let yy = spec.request.initialization.retained.destination_origin_m[1] + y;
                raw.push_str(&format!(
                    "{i},{j},{i},{j},{xx:.17e},{yy:.17e},{f:.17e},{h:.17e},{t:.17e},{phase:.17e}\n"
                ));
                energy += mass * h;
                water += mass * f;
                liquid += mass * f * phase;
            }
            let path = format!("cooling-{step}.csv");
            fs_write(root.path(), &path, raw.as_bytes());
            hashes.insert(
                path.clone(),
                serde_json::json!(format!("{:x}", Sha256::digest(raw.as_bytes()))),
            );
            snapshots.push(serde_json::json!({"path":path,"step":step,"time_s":step as f64*dt,"energy_j":energy,"water_mass_kg":water,"liquid_water_mass_kg":liquid,"cold_exchange_j":if step==0{0.}else{delta},"reflecting_exchange_j":0.}));
            observations.push(serde_json::json!({"step":step,"physical_time_s":step as f64*dt,"energy_j":energy,"water_mass_kg":water,"liquid_water_mass_kg":liquid,"solid_water_mass_kg":water-liquid,"minimum_temperature_k":t,"maximum_temperature_k":t}));
        }
        let native = serde_json::json!({"schema_version":1,"adapter":"OpenLB","backend":"cpu","precision":"float64","source_revision":"145cd54810b468f4b6fd3ed86b10644264841578","collision":"native_total_enthalpy_trt","trt_magic":0.25,"executed":true,"software_fallback":false,"request":spec.native_request().unwrap()["native_request"],"source_spacing_m":dx,"spacing_m":dx,"physical_step_s":dt,"cell_mass_kg":mass,"source_shape":spec.prepared.retained.extrusion.source_grid_shape,"native_shape":spec.prepared.retained.extrusion.source_grid_shape,"boundary":"native_half_link_cold_ymin; native_half_link_insulated_ymax; periodic_x","snapshots":snapshots,"physical_validation":"unqualified"});
        let native_raw = serde_json::to_vec(&native).unwrap();
        fs_write(
            root.path(),
            "native-retained-cooling-receipt.json",
            &native_raw,
        );
        let mut value = native;
        value["native_receipt_sha256"] =
            serde_json::json!(format!("{:x}", Sha256::digest(native_raw)));
        value["native_driver_sha256"] = serde_json::json!(format!(
            "{:x}",
            Sha256::digest(include_bytes!("../adapters/openlb_retained_cooling.cpp"))
        ));
        value["request_sha256"] = digest(&spec.native_request().unwrap()).unwrap().into();
        value["original_source_sha256"] = format!("{:x}", Sha256::digest(&source)).into();
        value["field_units"] =
            "position:m,water_fraction:1,specific_enthalpy:J/kg,temperature:K,liquid_fraction:1"
                .into();
        value["field_association"] = ASSOCIATION.into();
        value["boundary_exchange_units"] = "J".into();
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
        value["sandbox"] = serde_json::json!({"policy":crate::cooling_execution::SANDBOX_POLICY,"checks":serde_json::Map::from_iter(canaries.into_iter().map(|k|(k.to_string(),serde_json::json!(true))))});
        let checks = [
            "water_mass_relative_error",
            "initial_enthalpy_relative_error",
            "energy_balance_relative_error",
            "reflecting_exchange_relative_error",
            "phase_enthalpy_normalized_error",
        ];
        value["independent_verification"] = serde_json::json!({"checks":serde_json::Map::from_iter(checks.into_iter().map(|k|(k.to_string(),serde_json::json!({"error":0.,"tolerance":1e-10,"passed":true})))),"observations":observations,"original_files_sha256":hashes,"original_source_sha256":value["original_source_sha256"],"executed":false});
        (root, spec, source, value)
    }
    fn fs_write(root: &Path, path: &str, raw: &[u8]) {
        std::fs::write(root.join(path), raw).unwrap();
    }

    #[test]
    fn independent_complete_controls_and_boundary_energy_refuse_rehashed_corruption_and_changed_gates()
     {
        let (root, spec, source, value) = fixture();
        let verified = verify(&spec, &source, root.path(), &value).unwrap();
        assert!(verified.error < 1e-10);
        for key in [
            "request_sha256",
            "native_driver_sha256",
            "native_receipt_sha256",
            "sandbox",
            "snapshots",
            "original_source_sha256",
        ] {
            let mut changed = value.clone();
            changed[key] = Value::Null;
            assert!(
                verify(&spec, &source, root.path(), &changed).is_err(),
                "{key}"
            );
        }
        let mut changed = value.clone();
        changed["independent_verification"]["checks"]["energy_balance_relative_error"]["tolerance"] =
            serde_json::json!(0.02);
        assert!(verify(&spec, &source, root.path(), &changed).is_err());
        let path = root.path().join("heat-exchange.csv");
        let raw = std::fs::read(&path).unwrap();
        let altered = text(&raw)
            .unwrap()
            .replacen(",0\n", ",1e-10\n", 1)
            .into_bytes();
        std::fs::write(path, &altered).unwrap();
        changed = value.clone();
        changed["independent_verification"]["original_files_sha256"]["heat-exchange.csv"] =
            format!("{:x}", Sha256::digest(altered)).into();
        assert!(verify(&spec, &source, root.path(), &changed).is_err());
        fs_write(root.path(), "heat-exchange.csv", &raw);
        let path = root.path().join("cooling-0.csv");
        let raw = std::fs::read(&path).unwrap();
        let mut lines = text(&raw)
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let mut row = lines[1].split(',').map(str::to_string).collect::<Vec<_>>();
        row[2] = "1".into();
        lines[1] = row.join(",");
        let altered = lines.join("\n").into_bytes();
        std::fs::write(path, &altered).unwrap();
        changed = value.clone();
        changed["independent_verification"]["original_files_sha256"]["cooling-0.csv"] =
            format!("{:x}", Sha256::digest(altered)).into();
        assert!(verify(&spec, &source, root.path(), &changed).is_err());
    }
}
