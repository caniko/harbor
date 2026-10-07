//! Source-bound one-way extrusion of original native phase and velocity.

use crate::{
    Error, Result,
    contracts::{StageOperation, digest, invalid, token},
    qualification::{self, EvidenceRecord, EvidenceState},
    science::Quantity,
    storage::Store,
    wetting::WettingReferenceSpec,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WettingRetentionRequest {
    pub schema_version: u32,
    pub source_job: String,
    pub physical_time_s: f64,
    /// Explicit depth; the two-dimensional wetting simulation supplies no depth.
    pub extrusion: Quantity,
    pub extrusion_provenance: String,
    pub destination_region: String,
    /// Translation only. Native xy, phase and velocity distributions are retained.
    pub destination_origin_m: [f64; 3],
    pub maximum_relative_conservation_error: f64,
}
impl WettingRetentionRequest {
    pub fn validate(&self) -> Result<()> {
        let depth = self.extrusion.si("length")?;
        if self.schema_version != 1
            || uuid::Uuid::parse_str(&self.source_job).is_err()
            || !self.physical_time_s.is_finite()
            || self.physical_time_s < 0.
            || !depth.is_finite()
            || depth <= 0.
            || depth > 1.
            || self.extrusion_provenance.trim().is_empty()
            || self.extrusion_provenance.len() > 4096
            || !token(&self.destination_region)
            || self.destination_region.len() > 64
            || self
                .destination_origin_m
                .into_iter()
                .any(|v| !v.is_finite() || v.abs() > 1e6)
            || !self.maximum_relative_conservation_error.is_finite()
            || self.maximum_relative_conservation_error <= 0.
            || self.maximum_relative_conservation_error > 1e-10
        {
            return Err(invalid(
                "versioned exact-time wetting source, explicit positive SI extrusion/provenance, bounded translated region and unchanged conservative mass gate required",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WettingExtrusion {
    pub source_grid_shape: [usize; 2],
    pub native_step: u64,
    pub destination_cells: usize,
    pub destination_geometry_id: String,
    pub spacing_m: f64,
    pub extrusion_m: f64,
    pub point_control_volume_m3: f64,
    pub destination_control_bounds_m: [[f64; 2]; 3],
    pub density_kg_m3: f64,
    pub source_phase_area_m2: f64,
    pub source_phase_mass_kg: f64,
    pub destination_phase_mass_kg: f64,
    pub relative_conservation_error: f64,
    pub source_phase_fraction_range: [f64; 2],
    pub phase_fraction_bounds_satisfied: bool,
    pub nonphysical_phase_cells: usize,
    /// Signed negative phase amount; it is preserved rather than clipped away.
    pub negative_phase_mass_kg: f64,
    pub excess_phase_mass_kg: f64,
    pub velocity_scale_m_s: f64,
    pub maximum_speed_m_s: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RetainedWettingReport {
    pub schema_version: u32,
    pub initialization_id: String,
    pub request: WettingRetentionRequest,
    pub source_science_id: String,
    pub source_execution_id: String,
    pub source_execution_binding_digest: String,
    pub original_field: EvidenceRecord,
    pub original_receipt: EvidenceRecord,
    pub extrusion: WettingExtrusion,
    pub source_region: String,
    pub source_association: String,
    pub destination_association: String,
    pub orientation: [f64; 3],
    pub interpolation: String,
    pub phase_mapping: String,
    pub velocity_mapping: String,
    pub excluded_materials: Vec<u32>,
    pub preserved_original_distribution: bool,
    pub temperature_state: String,
    pub cooling_readiness: String,
    pub executed: bool,
    pub physical_validation: String,
    pub limitations: Vec<String>,
}

fn sum(values: impl IntoIterator<Item = f64>) -> f64 {
    let (mut total, mut correction) = (0., 0.);
    for value in values {
        let term = value - correction;
        let next = total + term;
        correction = (next - total) - term;
        total = next;
    }
    total
}

fn reconstruct(
    spec: &WettingReferenceSpec,
    request: &WettingRetentionRequest,
    bytes: &[u8],
) -> Result<WettingExtrusion> {
    request.validate()?;
    let step = spec
        .observation_steps
        .iter()
        .zip(spec.times_s())
        .find(|(_, time)| *time == request.physical_time_s)
        .map(|(step, _)| *step)
        .ok_or_else(|| {
            invalid("exact retained original wetting physical time required; no interpolation")
        })?;
    // Validate every node, geometry, material, phase and velocity before reduction.
    let complete = crate::wetting_fields::assess(spec, bytes)?;
    let dx = spec.spacing_m();
    let depth = request.extrusion.si("length")?;
    let volume = dx * dx * depth;
    let density = spec.density_liquid_kg_m3;
    let velocity_scale = dx / spec.physical_step_s();
    if ![volume, velocity_scale]
        .into_iter()
        .all(|v| v.is_finite() && v > 0.)
    {
        return Err(invalid(
            "finite positive extruded nodal volume and velocity conversion required",
        ));
    }
    let maximum_speed = complete.max_speed_lattice * velocity_scale;
    if !maximum_speed.is_finite() {
        return Err(invalid(
            "finite original physical velocity conversion required",
        ));
    }
    let mut phase = Vec::with_capacity(complete.fluid_nodes);
    let mut range = [f64::INFINITY, f64::NEG_INFINITY];
    for line in std::str::from_utf8(bytes)
        .map_err(|_| invalid("native wetting text required"))?
        .lines()
        .skip(1)
    {
        let row = line.split(',').collect::<Vec<_>>();
        if row[2] != "1" {
            continue;
        }
        let value = 1.
            - row[3]
                .parse::<f64>()
                .map_err(|_| invalid("original phase required"))?;
        range[0] = range[0].min(value);
        range[1] = range[1].max(value);
        phase.push(value);
    }
    if phase.len() != complete.fluid_nodes {
        return Err(invalid("complete active wetting nodal controls required"));
    }
    let source_mass = complete.droplet_area_m2 * density * depth;
    let destination_mass = sum(phase.iter().map(|fraction| fraction * density * volume));
    let error = (destination_mass / source_mass - 1.).abs();
    let negative = sum(phase
        .iter()
        .filter(|fraction| **fraction < 0.)
        .map(|fraction| fraction * density * volume));
    let excess = sum(phase
        .iter()
        .filter(|fraction| **fraction > 1.)
        .map(|fraction| (fraction - 1.) * density * volume));
    if !source_mass.is_finite()
        || source_mass <= 0.
        || !destination_mass.is_finite()
        || destination_mass <= 0.
        || !error.is_finite()
        || error > request.maximum_relative_conservation_error
        || !negative.is_finite()
        || !excess.is_finite()
    {
        return Err(invalid(
            "independent complete original phase area and extruded mass must conserve the unchanged approved integral",
        ));
    }
    let [nx, ny] = complete.shape;
    let [ox, oy, oz] = request.destination_origin_m;
    if [ox, oy, oz]
        .into_iter()
        .any(|origin| origin.abs() * f64::EPSILON > dx.min(depth) * 1e-8)
    {
        return Err(invalid(
            "translation must retain resolved native control-volume precision",
        ));
    }
    let bounds = [
        [ox - dx / 2., ox + (nx as f64 - 0.5) * dx],
        [oy + dx / 2., oy + (ny as f64 - 1.5) * dx],
        [oz, oz + depth],
    ];
    if bounds
        .iter()
        .any(|pair| pair.iter().any(|v| !v.is_finite()) || pair[0] >= pair[1])
    {
        return Err(invalid(
            "resolved finite extruded nodal control geometry required",
        ));
    }
    let geometry = digest(
        &serde_json::json!({"schema_version":1,"model":"native_uniform_nodal_extrusion",
        "native_shape":complete.shape,"material_profile":"walls_at_y0_and_ymax; all_x_points_active",
        "spacing_m":dx,"depth_m":depth,"origin_m":request.destination_origin_m,
        "bounds_m":bounds,"region":request.destination_region}),
    )?;
    Ok(WettingExtrusion {
        source_grid_shape: complete.shape,
        native_step: step,
        destination_cells: phase.len(),
        destination_geometry_id: geometry,
        spacing_m: dx,
        extrusion_m: depth,
        point_control_volume_m3: volume,
        destination_control_bounds_m: bounds,
        density_kg_m3: density,
        source_phase_area_m2: complete.droplet_area_m2,
        source_phase_mass_kg: source_mass,
        destination_phase_mass_kg: destination_mass,
        relative_conservation_error: error,
        source_phase_fraction_range: range,
        phase_fraction_bounds_satisfied: range[0] >= 0. && range[1] <= 1.,
        nonphysical_phase_cells: phase.iter().filter(|v| !(0. ..=1.).contains(*v)).count(),
        negative_phase_mass_kg: negative,
        excess_phase_mass_kg: excess,
        velocity_scale_m_s: velocity_scale,
        maximum_speed_m_s: maximum_speed,
    })
}

pub fn prepare(store: &Store, request: &WettingRetentionRequest) -> Result<RetainedWettingReport> {
    request.validate()?;
    let job = store.job(&request.source_job)?;
    let plan = store.recorded_plan(&request.source_job)?;
    let spec = plan.wetting.as_ref().ok_or_else(|| {
        Error::Unqualified("registered synthetic native wetting source required".into())
    })?;
    let evidence = qualification::inspect(store, &request.source_job)?;
    if job.state != "succeeded"
        || job.exit_code != Some(0)
        || !evidence.capabilities.iter().any(|capability| {
            capability.operation == StageOperation::WettingReference
                && matches!(capability.runtime_execution, EvidenceState::Recorded)
                && matches!(
                    capability.numerical_verification,
                    EvidenceState::ReportedPass
                )
        })
    {
        return Err(Error::Unqualified("complete succeeded execution-bound original-field wetting evidence required for retained state".into()));
    }
    let (original_receipt, _) = crate::results::registered(
        store,
        &request.source_job,
        "stages/wetting/verified-wetting-receipt.json",
    )?;
    let step = spec
        .observation_steps
        .iter()
        .zip(spec.times_s())
        .find(|(_, time)| *time == request.physical_time_s)
        .map(|(step, _)| *step)
        .ok_or_else(|| invalid("exact retained original wetting physical time required"))?;
    let path = format!("stages/wetting/wetting-{step}.csv");
    let (original_field, bytes) =
        crate::results::registered_bytes(store, &request.source_job, &path, "csv")?;
    let extrusion = reconstruct(spec, request, &bytes)?;
    let readiness = if extrusion.phase_fraction_bounds_satisfied {
        "missing_explicit_thermal_material_initial_state_and_cooling_model"
    } else {
        "phase_overshoots_require_explicit_resolution_before_physical_phase_initialization"
    };
    let mut report=RetainedWettingReport{schema_version:1,initialization_id:String::new(),request:request.clone(),
        source_science_id:plan.science_id()?,source_execution_id:job.plan_digest,
        source_execution_binding_digest:digest(&store.execution_binding(&request.source_job)?)?,original_field,original_receipt,extrusion,
        source_region:"complete_native_material_1".into(),source_association:"native_lattice_point_with_explicit_lumped_control_volume".into(),
        destination_association:"extruded_control_volume_cell".into(),orientation:[0.,0.,1.],
        interpolation:"identity_at_original_native_points; explicit_uniform_nodal_extrusion".into(),
        phase_mapping:"signed_phase_fraction=1-phi; phase_amount_density_kg_m3=rho_liquid*(1-phi); no clipping or thresholding".into(),
        velocity_mapping:"velocity_m_s=[u_lattice,v_lattice,0]*spacing_m/physical_step_s; translation preserves orientation".into(),
        excluded_materials:vec![2],preserved_original_distribution:true,temperature_state:"missing_from_native_wetting_source".into(),
        cooling_readiness:readiness.into(),executed:false,physical_validation:"unqualified".into(),limitations:vec![
            "synthetic equal-density/equal-viscosity diffuse phase amount; not qualified real water-air retention or ingress".into(),
            "all active native points carry dx^2*explicit_depth; control bounds retain the original nodal convention rather than clipping to the nominal box".into(),
            "original CSV remains authoritative for every phase and velocity; descriptor supplies a one-way identity/extrusion map, not a new interpolated field".into(),
            "no cooling execution, temperature invention, ice pressure, expansion, fracture or freeze-thaw lifetime inference".into()]};
    report.initialization_id = digest(&report)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> WettingRetentionRequest {
        WettingRetentionRequest {
            schema_version: 1,
            source_job: "00000000-0000-0000-0000-000000000001".into(),
            physical_time_s: 0.,
            extrusion: crate::science::Quantity {
                value: 1.,
                unit: "mm".into(),
            },
            extrusion_provenance: "explicit synthetic depth; not measured droplet volume".into(),
            destination_region: "retained_phase".into(),
            destination_origin_m: [0.01, 0.02, 0.03],
            maximum_relative_conservation_error: 1e-10,
        }
    }

    #[test]
    fn native_phase_extrusion_preserves_full_distribution_signed_mass_and_explicit_control_bounds()
    {
        let (_, plan, _, data) = crate::wetting::tests::fixture();
        let spec = plan.wetting.unwrap();
        let result = reconstruct(&spec, &request(), &data).unwrap();
        let original = crate::wetting_fields::assess(&spec, &data).unwrap();
        let independent_mass = original.droplet_area_m2; // rho=1000, depth=.001.
        assert!((result.destination_phase_mass_kg / independent_mass - 1.).abs() < 1e-12);
        assert!(result.relative_conservation_error < 1e-12);
        let [nx, ny] = result.source_grid_shape;
        assert_eq!(result.destination_cells, nx * (ny - 2));
        assert!(result.phase_fraction_bounds_satisfied);
        assert_eq!(result.nonphysical_phase_cells, 0);
        assert_eq!(
            result.velocity_scale_m_s,
            spec.spacing_m() / spec.physical_step_s()
        );
        assert_eq!(result.destination_control_bounds_m[2], [0.03, 0.031]);
        assert!(
            (result.destination_control_bounds_m[0][0] - (0.01 - spec.spacing_m() / 2.)).abs()
                < 1e-18
        );
        // Analytic tanh cap has a finite center value: retaining the original
        // Float64 forbids rounding the diffuse fraction into a pure liquid.
        assert_eq!(result.source_phase_fraction_range[1], 0.9999999999999823);
        let mut changed = request();
        changed.extrusion.value = 2.;
        let twice = reconstruct(&spec, &changed, &data).unwrap();
        assert!(
            (twice.destination_phase_mass_kg / result.destination_phase_mass_kg - 2.).abs() < 1e-14
        );
        assert_ne!(
            twice.destination_geometry_id,
            result.destination_geometry_id
        );
        changed = request();
        changed.destination_origin_m = [0., 0., 0.];
        let translated = reconstruct(&spec, &changed, &data).unwrap();
        assert_eq!(
            translated.destination_phase_mass_kg,
            result.destination_phase_mass_kg
        );
        assert_ne!(
            translated.destination_geometry_id,
            result.destination_geometry_id
        );
    }

    #[test]
    fn retention_rejects_unretained_time_missing_depth_invalid_units_and_corrupt_complete_fields() {
        let (_, plan, _, data) = crate::wetting::tests::fixture();
        let spec = plan.wetting.unwrap();
        let mut changed = request();
        changed.physical_time_s = spec.physical_step_s() / 2.;
        assert!(reconstruct(&spec, &changed, &data).is_err());
        for changed in [
            WettingRetentionRequest {
                extrusion: crate::science::Quantity {
                    value: 0.,
                    unit: "m".into(),
                },
                ..request()
            },
            WettingRetentionRequest {
                extrusion: crate::science::Quantity {
                    value: 1.,
                    unit: "kg".into(),
                },
                ..request()
            },
            WettingRetentionRequest {
                extrusion_provenance: String::new(),
                ..request()
            },
            WettingRetentionRequest {
                maximum_relative_conservation_error: 1e-3,
                ..request()
            },
        ] {
            assert!(changed.validate().is_err());
        }
        assert!(reconstruct(&spec, &request(), &data[..data.len() - 100]).is_err());
        let mut duplicated = data.clone();
        duplicated.extend_from_slice(data.split(|b| *b == b'\n').nth(10).unwrap());
        duplicated.push(b'\n');
        assert!(reconstruct(&spec, &request(), &duplicated).is_err());
        let mut far = request();
        far.destination_origin_m = [1e6, 0., 0.];
        assert!(reconstruct(&spec, &far, &data).is_err());
        // A supported diffuse phase overshoot is preserved, never clipped into
        // invented physical liquid. The cooling prerequisite remains explicit.
        let text = std::str::from_utf8(&data).unwrap();
        let lines = text
            .lines()
            .map(|line| {
                let mut row = line.split(',').map(str::to_owned).collect::<Vec<_>>();
                if row.len() == 6 && row[2] == "1" && row[3].parse::<f64>().is_ok_and(|p| p > 0.999)
                {
                    row[3] = "1.001".into();
                }
                row.join(",")
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        let retained = reconstruct(&spec, &request(), lines.as_bytes()).unwrap();
        assert!(!retained.phase_fraction_bounds_satisfied);
        assert!(retained.nonphysical_phase_cells > 0 && retained.negative_phase_mass_kg < 0.);
        assert!(
            retained.relative_conservation_error <= request().maximum_relative_conservation_error
        );
    }

    #[test]
    fn manufactured_unexecuted_wetting_receipts_cannot_become_authoritative_retained_initializations()
     {
        let (originals, plan, receipt, _) = crate::wetting::tests::fixture();
        let state = tempfile::tempdir().unwrap();
        let store = crate::storage::Store::open(&state.path().join("state")).unwrap();
        let job = store
            .submit(&plan, "unexecuted-retained-source-fixture")
            .unwrap();
        let root = store.job_dir(&job.id).unwrap();
        std::fs::create_dir_all(root.join("stages/wetting")).unwrap();
        let mut records = Vec::new();
        for step in &plan.wetting.as_ref().unwrap().observation_steps {
            let name = format!("wetting-{step}.csv");
            let path = format!("stages/wetting/{name}");
            std::fs::copy(originals.path().join(name), root.join(&path)).unwrap();
            records.push(
                crate::storage::native_manifest(
                    &root,
                    &path,
                    16 * 1024 * 1024,
                    "unexecuted fixture",
                )
                .unwrap(),
            );
        }
        crate::wetting::annotate_fields(&plan, &mut records).unwrap();
        for record in records {
            store.add_artifact(&job.id, &record).unwrap();
        }
        let record = crate::storage::commit_artifact(
            &root,
            "stages/wetting/verified-wetting-receipt.json",
            &serde_json::to_vec(&receipt).unwrap(),
            "json",
            "unexecuted fixture",
        )
        .unwrap();
        store.add_artifact(&job.id, &record).unwrap();
        let mut requested = request();
        requested.source_job = job.id;
        assert!(matches!(
            prepare(&store, &requested),
            Err(Error::Unqualified(_))
        ));
        let database = std::fs::read(store.root.join("jobs.sqlite3")).unwrap();
        assert!(prepare(&store, &requested).is_err());
        assert_eq!(
            std::fs::read(store.root.join("jobs.sqlite3")).unwrap(),
            database
        );
    }
}
