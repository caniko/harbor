//! Explicit one-way transient-temperature to static planar-contact recipe.
use crate::{
    Result, contact::ContactReferenceSpec, contracts::invalid,
    moisture_results::NativeMoistureAssessment, thermal::ThermalReferenceSpec,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Mechanical inputs only: final temperatures are derived from native fields.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContactMechanics {
    pub size_m: [f64; 3],
    pub resolution: u32,
    pub geometry_tolerance_m: f64,
    pub initial_gap_m: f64,
    pub preload_compression_m: f64,
    pub final_compression_m: f64,
    pub young_modulus_pa: [f64; 2],
    pub expansion_per_k: [f64; 2],
    pub reference_temperature_k: f64,
    pub contact_stiffness_pa_m: f64,
    pub numerical_tolerance: f64,
    pub material_provenance: String,
    pub contact_provenance: String,
    pub boundary_provenance: String,
}
impl ContactMechanics {
    pub fn reference(&self, temperatures_k: [f64; 2]) -> Result<ContactReferenceSpec> {
        let spec = ContactReferenceSpec {
            schema_version: 1,
            synthetic: true,
            backend: "cpu".into(),
            formulation: "planar_linear_penalty_contact".into(),
            size_m: self.size_m,
            resolution: self.resolution,
            geometry_tolerance_m: self.geometry_tolerance_m,
            initial_gap_m: self.initial_gap_m,
            preload_compression_m: self.preload_compression_m,
            final_compression_m: self.final_compression_m,
            young_modulus_pa: self.young_modulus_pa,
            expansion_per_k: self.expansion_per_k,
            reference_temperature_k: self.reference_temperature_k,
            final_temperatures_k: temperatures_k,
            contact_stiffness_pa_m: self.contact_stiffness_pa_m,
            numerical_tolerance: self.numerical_tolerance,
            material_provenance: self.material_provenance.clone(),
            contact_provenance: self.contact_provenance.clone(),
            boundary_provenance: self.boundary_provenance.clone(),
        };
        spec.validate()?;
        Ok(spec)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThermalContactSpec {
    pub schema_version: u32,
    pub synthetic: bool,
    /// Independent lower/upper block histories. There is no feedback of contact
    /// pressure/gap into thermal interface conductance in this one-way model.
    pub thermal: [ThermalReferenceSpec; 2],
    pub mechanical: ContactMechanics,
    pub coupling_time_s: f64,
    pub maximum_projection_error_k: [f64; 2],
    pub maximum_relative_conservation_error: f64,
    pub moisture_risk: NativeMoistureAssessment,
    pub coupling_provenance: String,
}
impl ThermalContactSpec {
    pub fn validate(&self) -> Result<()> {
        let m = &self.mechanical;
        m.reference([m.reference_temperature_k; 2])?;
        if self.schema_version != 1
            || !self.synthetic
            || !self.coupling_time_s.is_finite()
            || self.coupling_time_s <= 0.
            || self
                .maximum_projection_error_k
                .iter()
                .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
            || !self.maximum_relative_conservation_error.is_finite()
            || self.maximum_relative_conservation_error <= 0.
            || self.maximum_relative_conservation_error > 1e-10
            || self.coupling_provenance.trim().is_empty()
            || self.coupling_provenance.len() > 4096
        {
            return Err(invalid(
                "explicit synthetic one-way coupling, exact retained time, bounded Kelvin projection loss and conservative gate required",
            ));
        }
        for (block, thermal) in self.thermal.iter().enumerate() {
            thermal.validate()?;
            if thermal.size_m != m.size_m
                || thermal.geometry_tolerance_m != m.geometry_tolerance_m
                || thermal.initial_temperature_k != m.reference_temperature_k
                || !thermal.observation_times_s.contains(&self.coupling_time_s)
                || thermal.material_temperature_domain_k.iter().any(|t| {
                    !(100. ..=1000.).contains(t)
                        || (m.expansion_per_k[block] * (t - m.reference_temperature_k)).abs()
                            > 0.001
                })
            {
                return Err(invalid(
                    "congruent native thermal/contact boxes, same geometry tolerance and stress-free initial reference, exact retained coupling time and complete small-strain material range required",
                ));
            }
        }
        // Validates the declared air/missing/inapplicable branch, never a
        // caller-provided surface temperature. Actual screening is post-solve.
        self.moisture_risk.inspect(m.reference_temperature_k)?;
        Ok(())
    }
}
