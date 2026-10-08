//! Prescribed full-face dry snow as an explicitly gated quasi-steady resistance.
//! The existing native plane-wall solver owns the transient device conduction.
use crate::{
    Result,
    contracts::{digest, invalid},
    science::Quantity,
    thermal::ThermalReferenceSpec,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

const PREFIX: &str = "harbor-cad-snow-boundary-v1:";

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SnowPrescription {
    pub schema_version: u32,
    pub synthetic: bool,
    pub model: String,
    pub coverage: String,
    pub opening_model: String,
    pub thickness: Quantity,
    pub conductivity: Quantity,
    pub density: Quantity,
    pub specific_heat: Quantity,
    pub material_temperature_domain_k: [f64; 2],
    pub maximum_omitted_capacity_ratio: f64,
    pub maximum_diffusion_timescale_ratio: f64,
    pub provenance: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SnowReferenceSpec {
    pub schema_version: u32,
    /// Bare-wall thermal history and explicitly prescribed air-side coefficient.
    pub thermal: ThermalReferenceSpec,
    pub snow: SnowPrescription,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreparedSnowBoundary {
    pub schema_version: u32,
    pub preparation_id: String,
    pub input: SnowReferenceSpec,
    pub native: ThermalReferenceSpec,
    pub thickness_m: f64,
    pub snow_resistance_m2_k_w: f64,
    pub bare_air_resistance_m2_k_w: f64,
    pub effective_convection_w_m2_k: f64,
    pub covered_area_m2: f64,
    pub snow_mass_kg: f64,
    pub snow_capacity_j_k: f64,
    pub omitted_capacity_ratio: f64,
    pub snow_diffusion_time_s: f64,
    pub shortest_forcing_or_observation_interval_s: f64,
    pub diffusion_timescale_ratio: f64,
    pub initial_outer_snow_temperature_k: f64,
    pub initial_heat_loss_w: f64,
    pub executed: bool,
    pub physical_validation: String,
    pub limitations: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    schema_version: u32,
    snow: SnowPrescription,
    bare_convection_w_m2_k: f64,
    bare_convection_provenance: String,
}

impl SnowReferenceSpec {
    pub fn plan(&self, policy: String) -> Result<serde_json::Value> {
        let prepared = self.prepare()?;
        let plan =
            crate::contracts::ExecutionPlan::thermal_reference(prepared.native.clone(), policy)?;
        Ok(serde_json::json!({"approval_digest":plan.id()?,"plan":plan,"snow_boundary":prepared}))
    }

    pub fn prepare(&self) -> Result<PreparedSnowBoundary> {
        let prepared = self.derive()?;
        prepared.native.validate()?;
        Ok(prepared)
    }

    fn derive(&self) -> Result<PreparedSnowBoundary> {
        let positive = |v: f64| v.is_finite() && v > 0.;
        let snow = &self.snow;
        if self.schema_version != 1
            || snow.schema_version != 1
            || !snow.synthetic
            || snow.model != "dry_quasi_steady_series_resistance"
            || snow.coverage != "complete_both_x_faces"
            || snow.opening_model != "no_openings_in_plane_wall_fixture"
            || snow.provenance.trim().is_empty()
            || snow.provenance.len() > 4096
            || self.thermal.convection_provenance.starts_with(PREFIX)
            || [
                snow.maximum_omitted_capacity_ratio,
                snow.maximum_diffusion_timescale_ratio,
            ]
            .into_iter()
            .any(|v| !positive(v) || v > 0.02)
        {
            return Err(invalid(
                "explicit synthetic full-face dry snow, no openings, bounded quasi-steady applicability and provenance required",
            ));
        }
        self.thermal.validate()?;
        let d = snow.thickness.si("length")?;
        let k = snow.conductivity.si("thermal_conductivity")?;
        let rho = snow.density.si("density")?;
        let cp = snow.specific_heat.si("specific_heat")?;
        let h = self.thermal.convection_w_m2_k;
        if ![d, k, rho, cp, h].into_iter().all(positive) || d > self.thermal.size_m[0] {
            return Err(invalid(
                "positive explicit dry-snow thermal properties, resolved thickness and bare air-side coefficient required",
            ));
        }
        let capacity = self.thermal.density_kg_m3
            * self.thermal.specific_heat_j_kg_k
            * self.thermal.size_m.iter().product::<f64>();
        let upper = self
            .thermal
            .ambient_history
            .iter()
            .map(|p| p[1])
            .fold(self.thermal.initial_temperature_k, f64::max)
            + self.thermal.heater_energy(self.thermal.duration_s)? / capacity;
        let lower = self
            .thermal
            .ambient_history
            .iter()
            .map(|p| p[1])
            .fold(self.thermal.initial_temperature_k, f64::min);
        let domain = snow.material_temperature_domain_k;
        if !domain.into_iter().all(positive)
            || domain[0] >= domain[1]
            || domain[1] >= 273.15
            || !upper.is_finite()
            || upper >= 273.15
            || upper > domain[1]
            || lower < domain[0]
        {
            return Err(invalid(
                "conservative complete-history temperature bound must stay in the prescribed dry subzero snow domain; melting and latent heat require a different model",
            ));
        }
        let snow_resistance = d / k;
        let air_resistance = 1. / h;
        let effective = 1. / (snow_resistance + air_resistance);
        let area = 2. * self.thermal.size_m[1] * self.thermal.size_m[2];
        let mass = rho * area * d;
        let snow_capacity = mass * cp;
        let capacity_ratio = snow_capacity / capacity;
        let diffusion_time = rho * cp * d * d / k;
        let mut shortest = self.thermal.duration_s;
        for history in [&self.thermal.ambient_history, &self.thermal.heater_history] {
            for interval in history.windows(2) {
                shortest = shortest.min(interval[1][0] - interval[0][0]);
            }
        }
        let mut last = 0.;
        for time in &self.thermal.observation_times_s {
            shortest = shortest.min(*time - last);
            last = *time;
        }
        let time_ratio = diffusion_time / shortest;
        let outer = self.thermal.ambient_history[0][1]
            + (self.thermal.initial_temperature_k - self.thermal.ambient_history[0][1])
                * air_resistance
                / (air_resistance + snow_resistance);
        let initial_loss = area
            * effective
            * (self.thermal.initial_temperature_k - self.thermal.ambient_history[0][1]);
        if ![
            snow_resistance,
            air_resistance,
            effective,
            area,
            mass,
            snow_capacity,
            capacity_ratio,
            diffusion_time,
            shortest,
            time_ratio,
        ]
        .into_iter()
        .all(positive)
            || !outer.is_finite()
            || !initial_loss.is_finite()
            || capacity_ratio > snow.maximum_omitted_capacity_ratio
            || time_ratio > snow.maximum_diffusion_timescale_ratio
        {
            return Err(invalid(
                "finite snow series resistance and unchanged omitted-capacity/diffusion-timescale applicability gates required",
            ));
        }
        let binding = Binding {
            schema_version: 1,
            snow: snow.clone(),
            bare_convection_w_m2_k: h,
            bare_convection_provenance: self.thermal.convection_provenance.clone(),
        };
        let mut native = self.thermal.clone();
        native.convection_w_m2_k = effective;
        native.convection_provenance = format!("{PREFIX}{}", serde_json::to_string(&binding)?);
        if native.convection_provenance.len() > 4096 {
            return Err(invalid(
                "bounded original snow prescription and bare coefficient provenance required",
            ));
        }
        Ok(PreparedSnowBoundary{schema_version:1,preparation_id:digest(self)?,input:self.clone(),native,
            thickness_m:d,snow_resistance_m2_k_w:snow_resistance,bare_air_resistance_m2_k_w:air_resistance,
            effective_convection_w_m2_k:effective,covered_area_m2:area,snow_mass_kg:mass,snow_capacity_j_k:snow_capacity,
            omitted_capacity_ratio:capacity_ratio,snow_diffusion_time_s:diffusion_time,shortest_forcing_or_observation_interval_s:shortest,
            diffusion_timescale_ratio:time_ratio,initial_outer_snow_temperature_k:outer,initial_heat_loss_w:initial_loss,
            executed:false,physical_validation:"unqualified".into(),limitations:vec![
                "prescribed dry uniform full coverage; no deposition, adhesion or snow evolution".into(),
                "snow heat storage omitted under explicit small-capacity and slow-forcing applicability screens; screens are not physical validation".into(),
                "quasi-steady linear snow profile initialized between the prescribed device and air temperatures".into(),
                "no melting, radiation, ice/frost transport or contact resistance".into(),
                "plane-wall fixture has no openings; blocked-opening flow geometry is a separate capability".into()]})
    }
}

/// Reconstruct the original prescription rather than trusting its provenance text.
/// Old unannotated plane-wall inputs keep their existing serialization/semantics.
pub(crate) fn verify_binding(
    native: &ThermalReferenceSpec,
) -> Result<Option<PreparedSnowBoundary>> {
    let Some(text) = native.convection_provenance.strip_prefix(PREFIX) else {
        return Ok(None);
    };
    let binding: Binding = serde_json::from_str(text)?;
    if binding.schema_version != 1 {
        return Err(invalid("supported snow boundary binding version required"));
    }
    let mut bare = native.clone();
    bare.convection_w_m2_k = binding.bare_convection_w_m2_k;
    bare.convection_provenance = binding.bare_convection_provenance;
    let expected = SnowReferenceSpec {
        schema_version: 1,
        thermal: bare,
        snow: binding.snow,
    }
    .derive()?;
    if digest(&expected.native)? != digest(native)? {
        return Err(invalid(
            "native snow boundary differs from the exact original prescribed series resistance",
        ));
    }
    Ok(Some(expected))
}
