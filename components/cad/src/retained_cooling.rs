//! Explicit source-bound enthalpy initialization for stationary retained phase.
use crate::{
    Result,
    contracts::{digest, invalid},
    moisture_results::NativeMoistureAssessment,
    science::Quantity,
    storage::Store,
    wetting::WettingReferenceSpec,
    wetting_retention::{RetainedWettingReport, WettingRetentionRequest},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RetainedCoolingInputs {
    pub schema_version: u32,
    pub synthetic: bool,
    pub formulation: String,
    pub density: Quantity,
    pub specific_heat: Quantity,
    pub conductivity: Quantity,
    pub latent_heat_j_kg: f64,
    pub melting_temperature: Quantity,
    pub initial_temperature: Quantity,
    pub cold_wall_temperature: Quantity,
    pub material_temperature_domain: [Quantity; 2],
    pub material_provenance: String,
    pub initial_temperature_provenance: String,
    pub cooling_boundary_provenance: String,
    pub maximum_relative_conservation_error: f64,
    pub moisture_risk: NativeMoistureAssessment,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CoolingSiInputs {
    pub density_kg_m3: f64,
    pub specific_heat_j_kg_k: f64,
    pub conductivity_w_m_k: f64,
    pub latent_heat_j_kg: f64,
    pub melting_temperature_k: f64,
    pub initial_temperature_k: f64,
    pub cold_wall_temperature_k: f64,
    pub material_temperature_domain_k: [f64; 2],
    pub stefan_number_at_full_water_fraction: f64,
}

impl RetainedCoolingInputs {
    pub fn normalize(&self) -> Result<CoolingSiInputs> {
        let rho = self.density.si("density")?;
        let cp = self.specific_heat.si("specific_heat")?;
        let k = self.conductivity.si("thermal_conductivity")?;
        let tm = self.melting_temperature.si("temperature")?;
        let initial = self.initial_temperature.si("temperature")?;
        let cold = self.cold_wall_temperature.si("temperature")?;
        let domain = [
            self.material_temperature_domain[0].si("temperature")?,
            self.material_temperature_domain[1].si("temperature")?,
        ];
        let stefan = cp * (tm - cold) / self.latent_heat_j_kg;
        if self.schema_version != 1
            || !self.synthetic
            || self.formulation != "stationary_equal_property_retained_phase_conduction"
            || [rho, cp, k, self.latent_heat_j_kg]
                .into_iter()
                .any(|v| !v.is_finite() || v <= 0.)
            || [tm, initial, cold, domain[0], domain[1]]
                .into_iter()
                .any(|v| !(100. ..=1000.).contains(&v))
            || domain[0] >= domain[1]
            || cold < domain[0]
            || tm > domain[1]
            || cold >= tm
            || initial != tm
            || !(0.05..=0.2).contains(&stefan)
            || !self.maximum_relative_conservation_error.is_finite()
            || self.maximum_relative_conservation_error <= 0.
            || self.maximum_relative_conservation_error > 1e-10
            || [
                &self.material_provenance,
                &self.initial_temperature_provenance,
                &self.cooling_boundary_provenance,
            ]
            .into_iter()
            .any(|p| p.trim().is_empty() || p.len() > 4096)
        {
            return Err(invalid(
                "explicit synthetic equal-property material/domain, initial melting temperature, cold boundary/provenance and unchanged conservative enthalpy gate required",
            ));
        }
        self.moisture_risk.inspect(cold)?;
        Ok(CoolingSiInputs {
            density_kg_m3: rho,
            specific_heat_j_kg_k: cp,
            conductivity_w_m_k: k,
            latent_heat_j_kg: self.latent_heat_j_kg,
            melting_temperature_k: tm,
            initial_temperature_k: initial,
            cold_wall_temperature_k: cold,
            material_temperature_domain_k: domain,
            stefan_number_at_full_water_fraction: stefan,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RetainedCoolingRequest {
    pub schema_version: u32,
    pub retained: WettingRetentionRequest,
    pub thermal: RetainedCoolingInputs,
}
impl RetainedCoolingRequest {
    pub fn validate(&self) -> Result<()> {
        self.retained.validate()?;
        self.thermal.normalize()?;
        if self.schema_version != 1 {
            return Err(invalid(
                "versioned conservative retained cooling request required",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CoolingInitialization {
    pub original_active_controls: usize,
    pub complete_thermal_mass_kg: f64,
    pub conserved_water_mass_kg: f64,
    pub sensible_initial_energy_j: f64,
    pub water_latent_initial_energy_j: f64,
    pub initial_total_energy_j: f64,
    pub independently_mapped_total_energy_j: f64,
    pub relative_initial_energy_error: f64,
    pub thermal_physical_step_s: f64,
    pub original_velocity_state: String,
    pub original_fraction_state: String,
    pub latent_mapping: String,
    pub energy_zero: String,
}

fn sum(values: impl Iterator<Item = f64>) -> f64 {
    let (mut total, mut correction) = (0., 0.);
    for v in values {
        let term = v - correction;
        let next = total + term;
        correction = (next - total) - term;
        total = next;
    }
    total
}

pub(crate) fn reconstruct(
    spec: &WettingReferenceSpec,
    request: &RetainedCoolingRequest,
    bytes: &[u8],
) -> Result<CoolingInitialization> {
    request.validate()?;
    let thermal = request.thermal.normalize()?;
    let retained = crate::wetting_retention::reconstruct(spec, &request.retained, bytes)?;
    if !retained.phase_fraction_bounds_satisfied
        || retained.maximum_speed_m_s != 0.
        || thermal.density_kg_m3 != spec.density_liquid_kg_m3
        || spec.density_liquid_kg_m3 != spec.density_vapor_kg_m3
    {
        return Err(invalid(
            "unchanged bounded original phase, exactly stationary original velocity and source-matching equal density required; no clipping, flow suppression or density substitution",
        ));
    }
    let mass = thermal.density_kg_m3 * retained.point_control_volume_m3;
    let sensible_h = thermal.specific_heat_j_kg_k
        * (thermal.initial_temperature_k - thermal.cold_wall_temperature_k);
    let mut fractions = Vec::with_capacity(retained.destination_cells);
    for line in std::str::from_utf8(bytes)
        .map_err(|_| invalid("complete original retained field text required"))?
        .lines()
        .skip(1)
    {
        let values = line.split(',').collect::<Vec<_>>();
        if values[2] == "1" {
            fractions.push(
                1. - values[3]
                    .parse::<f64>()
                    .map_err(|_| invalid("original phase fraction required"))?,
            );
        }
    }
    let thermal_mass = retained.destination_cells as f64 * mass;
    let sensible = thermal_mass * sensible_h;
    let latent = retained.source_phase_mass_kg * thermal.latent_heat_j_kg;
    let total = sensible + latent;
    let mapped = sum(fractions
        .into_iter()
        .map(|f| mass * (sensible_h + f * thermal.latent_heat_j_kg)));
    let error = (mapped / total - 1.).abs();
    let dt = retained.spacing_m.powi(2) * thermal.density_kg_m3 * thermal.specific_heat_j_kg_k
        / (6. * thermal.conductivity_w_m_k);
    if [thermal_mass, sensible, latent, total, mapped, dt]
        .into_iter()
        .any(|v| !v.is_finite() || v <= 0.)
        || !error.is_finite()
        || error > request.thermal.maximum_relative_conservation_error
    {
        return Err(invalid(
            "finite complete original-control mass and initial enthalpy must satisfy the independent unchanged conservative gate",
        ));
    }
    Ok(CoolingInitialization {
        original_active_controls: retained.destination_cells,
        complete_thermal_mass_kg: thermal_mass,
        conserved_water_mass_kg: retained.source_phase_mass_kg,
        sensible_initial_energy_j: sensible,
        water_latent_initial_energy_j: latent,
        initial_total_energy_j: total,
        independently_mapped_total_energy_j: mapped,
        relative_initial_energy_error: error,
        thermal_physical_step_s: dt,
        original_velocity_state: "exactly_zero_preserved".into(),
        original_fraction_state: "complete_original_bounded_unclipped".into(),
        latent_mapping:
            "per_original_control_f_times_L; equal_property_background_has_no_latent_heat".into(),
        energy_zero:
            "complete solid/background material at explicitly prescribed cold wall temperature"
                .into(),
    })
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreparedRetainedCooling {
    pub schema_version: u32,
    pub initialization_id: String,
    pub request: RetainedCoolingRequest,
    pub retained: RetainedWettingReport,
    pub normalized: CoolingSiInputs,
    pub initialization: CoolingInitialization,
    pub executed: bool,
    pub physical_validation: String,
    pub readiness: String,
}

pub fn prepare(store: &Store, request: RetainedCoolingRequest) -> Result<PreparedRetainedCooling> {
    request.validate()?;
    let retained = crate::wetting_retention::prepare(store, &request.retained)?;
    let plan = store.recorded_plan(&request.retained.source_job)?;
    let spec = plan
        .wetting
        .as_ref()
        .ok_or_else(|| invalid("registered original wetting recipe required"))?;
    let (_, bytes) = crate::results::registered_bytes(
        store,
        &request.retained.source_job,
        &retained.original_field.path,
        "csv",
    )?;
    let normalized = request.thermal.normalize()?;
    let initialization = reconstruct(spec, &request, &bytes)?;
    let mut report = PreparedRetainedCooling {
        schema_version: 1,
        initialization_id: String::new(),
        request,
        retained,
        normalized,
        initialization,
        executed: false,
        physical_validation: "unqualified".into(),
        readiness:
            "conservative_initial_enthalpy_prepared; independent_native_cooling_approval_required"
                .into(),
    };
    report.initialization_id = digest(&report)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn q(value: f64, unit: &str) -> Quantity {
        Quantity {
            value,
            unit: unit.into(),
        }
    }
    fn request() -> RetainedCoolingRequest {
        RetainedCoolingRequest {schema_version:1,
            retained:WettingRetentionRequest {schema_version:1,source_job:"00000000-0000-0000-0000-000000000001".into(),physical_time_s:0.,extrusion:q(1.,"mm"),extrusion_provenance:"manufactured explicit extrusion".into(),destination_region:"retained_phase".into(),destination_origin_m:[0.,0.,0.],maximum_relative_conservation_error:1e-10},
            thermal:RetainedCoolingInputs {schema_version:1,synthetic:true,formulation:"stationary_equal_property_retained_phase_conduction".into(),density:q(1000.,"kg/m3"),specific_heat:q(4180.,"J/(kg*K)"),conductivity:q(0.6,"W/(m*K)"),latent_heat_j_kg:334400.,melting_temperature:q(0.,"degC"),initial_temperature:q(273.15,"K"),cold_wall_temperature:q(-8.,"degC"),material_temperature_domain:[q(250.,"K"),q(300.,"K")],material_provenance:"manufactured equal properties for background and solid/liquid; not real air".into(),initial_temperature_provenance:"explicit independently prescribed initial melting state; wetting supplies no temperature".into(),cooling_boundary_provenance:"manufactured cold bottom, insulated top and periodic x".into(),maximum_relative_conservation_error:1e-10,moisture_risk:NativeMoistureAssessment::Missing{reason:"no measured humidity/history supplied".into()}}}
    }
    #[test]
    fn original_nonuniform_phase_conserves_water_mass_and_complete_sensible_plus_latent_enthalpy() {
        let (_, plan, _, data) = crate::wetting::tests::fixture();
        let spec = plan.wetting.unwrap();
        let mut request = request();
        request.thermal.density.value = spec.density_liquid_kg_m3;
        let report = reconstruct(&spec, &request, &data).unwrap();
        let retained =
            crate::wetting_retention::reconstruct(&spec, &request.retained, &data).unwrap();
        assert_eq!(
            report.conserved_water_mass_kg,
            retained.source_phase_mass_kg
        );
        assert!(report.complete_thermal_mass_kg > report.conserved_water_mass_kg);
        assert_eq!(
            report.water_latent_initial_energy_j,
            report.conserved_water_mass_kg * 334400.
        );
        assert!(report.relative_initial_energy_error <= 1e-10);
        assert_eq!(
            report.initial_total_energy_j,
            report.sensible_initial_energy_j + report.water_latent_initial_energy_j
        );
        let mut translated = request.clone();
        translated.retained.destination_origin_m = [0.01, 0.02, 0.03];
        assert_eq!(
            reconstruct(&spec, &translated, &data)
                .unwrap()
                .initial_total_energy_j,
            report.initial_total_energy_j
        );
        assert_ne!(digest(&request).unwrap(), digest(&translated).unwrap());
    }
    #[test]
    fn incomplete_thermal_states_moving_originals_and_signed_phase_cannot_initialize_stationary_cooling()
     {
        let (_, plan, _, data) = crate::wetting::tests::fixture();
        let spec = plan.wetting.unwrap();
        let mut request = request();
        request.thermal.density.value = spec.density_liquid_kg_m3;
        for field in [
            "initial_temperature_provenance",
            "material_provenance",
            "cooling_boundary_provenance",
        ] {
            let mut changed = serde_json::to_value(&request).unwrap();
            changed["thermal"][field] = "".into();
            assert!(
                serde_json::from_value::<RetainedCoolingRequest>(changed)
                    .unwrap()
                    .validate()
                    .is_err()
            );
        }
        let mut changed = request.clone();
        changed.thermal.initial_temperature.value += 1.;
        assert!(changed.validate().is_err());
        let mut changed = request.clone();
        changed.thermal.density.value *= 2.;
        assert!(reconstruct(&spec, &changed, &data).is_err());
        let mut changed = request.clone();
        changed.thermal.maximum_relative_conservation_error = 0.02;
        assert!(changed.validate().is_err());
        let raw = std::str::from_utf8(&data).unwrap();
        let mut rows = raw.lines().map(str::to_owned).collect::<Vec<_>>();
        let index = rows
            .iter()
            .position(|line| line.split(',').nth(2) == Some("1"))
            .unwrap();
        let mut row = rows[index]
            .split(',')
            .map(str::to_owned)
            .collect::<Vec<_>>();
        row[4] = "1e-9".into();
        rows[index] = row.join(",");
        assert!(reconstruct(&spec, &request, rows.join("\n").as_bytes()).is_err());
        row[4] = "0".into();
        row[3] = "1.0000000000000002".into();
        rows[index] = row.join(",");
        assert!(reconstruct(&spec, &request, rows.join("\n").as_bytes()).is_err());
        let mut changed = serde_json::to_value(request).unwrap();
        changed["thermal"]["latent_temperature"] = serde_json::Value::Null;
        assert!(serde_json::from_value::<RetainedCoolingRequest>(changed).is_err());
    }
}
