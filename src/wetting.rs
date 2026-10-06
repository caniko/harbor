//! Explicit synthetic planar diffuse-interface wetting, independent of airflow.
use crate::{Result, contracts::invalid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WettingReferenceSpec {
    pub schema_version: u32,
    pub synthetic: bool,
    pub backend: String,
    pub formulation: String,
    pub diameter_m: f64,
    pub resolution: u32,
    pub interface_width_m: f64,
    pub density_liquid_kg_m3: f64,
    pub density_vapor_kg_m3: f64,
    pub viscosity_liquid_m2_s: f64,
    pub viscosity_vapor_m2_s: f64,
    pub surface_tension_n_m: f64,
    pub contact_angle_deg: f64,
    pub phase_relaxation_time: f64,
    pub steps: u64,
    pub observation_steps: Vec<u64>,
    pub mass_tolerance: f64,
    pub angle_tolerance_deg: f64,
    pub material_provenance: String,
    pub boundary_provenance: String,
}

impl WettingReferenceSpec {
    pub fn spacing_m(&self) -> f64 {
        self.diameter_m / f64::from(self.resolution)
    }
    pub fn physical_step_s(&self) -> f64 {
        0.5 / 3. * self.spacing_m().powi(2) / self.viscosity_liquid_m2_s
    }
    pub fn surface_tension_lattice(&self) -> f64 {
        self.surface_tension_n_m * self.physical_step_s().powi(2)
            / (self.density_liquid_kg_m3 * self.spacing_m().powi(3))
    }
    pub fn times_s(&self) -> Vec<f64> {
        self.observation_steps
            .iter()
            .map(|step| *step as f64 * self.physical_step_s())
            .collect()
    }
    pub fn validate(&self) -> Result<()> {
        let positive = |v: f64| v.is_finite() && v > 0.;
        if self.schema_version != 1
            || !self.synthetic
            || self.backend != "cpu"
            || self.formulation != "well_balanced_contact_angle_2d"
            || !(24..=96).contains(&self.resolution)
            || !(100..=200000).contains(&self.steps)
            || !positive(self.diameter_m)
            || self.diameter_m > 0.001
            || !positive(self.density_liquid_kg_m3)
            || self.density_liquid_kg_m3 > 1e5
            || self.density_liquid_kg_m3 != self.density_vapor_kg_m3
            || !positive(self.viscosity_liquid_m2_s)
            || self.viscosity_liquid_m2_s > 1.
            || self.viscosity_liquid_m2_s != self.viscosity_vapor_m2_s
            || !positive(self.surface_tension_n_m)
            || !positive(self.surface_tension_lattice())
            || self.surface_tension_lattice() > 0.02
            || !positive(self.physical_step_s())
            || !positive(self.interface_width_m)
            || self.interface_width_m > self.diameter_m / 6.
            || !(3. ..=16.).contains(&(self.interface_width_m / self.spacing_m()))
            || !(60. ..=120.).contains(&self.contact_angle_deg)
            || !(0.6..=1.5).contains(&self.phase_relaxation_time)
            || !positive(self.mass_tolerance)
            || self.mass_tolerance > 1e-3
            || !positive(self.angle_tolerance_deg)
            || self.angle_tolerance_deg > 5.
            || !(2..=32).contains(&self.observation_steps.len())
            || self.observation_steps.first() != Some(&0)
            || self.observation_steps.last() != Some(&self.steps)
            || self
                .observation_steps
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            || [&self.material_provenance, &self.boundary_provenance]
                .iter()
                .any(|v| v.trim().is_empty() || v.len() > 4096)
        {
            return Err(invalid(
                "explicit bounded synthetic equal-property CPU planar wetting with resolved interface, SI inputs, provenance and unchanged mass/contact-angle gates required",
            ));
        }
        Ok(())
    }
}
