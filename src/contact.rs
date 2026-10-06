//! Explicit SI synthetic two-state planar penalty-contact reference contract.
use crate::{Result, contracts::invalid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContactReferenceSpec {
    pub schema_version: u32,
    pub synthetic: bool,
    pub backend: String,
    pub formulation: String,
    pub size_m: [f64; 3],
    pub resolution: u32,
    pub geometry_tolerance_m: f64,
    pub initial_gap_m: f64,
    pub preload_compression_m: f64,
    pub final_compression_m: f64,
    pub young_modulus_pa: [f64; 2],
    pub expansion_per_k: [f64; 2],
    pub reference_temperature_k: f64,
    pub final_temperatures_k: [f64; 2],
    pub contact_stiffness_pa_m: f64,
    pub numerical_tolerance: f64,
    pub material_provenance: String,
    pub contact_provenance: String,
    pub boundary_provenance: String,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct ContactReferenceState {
    pub solver_step_parameter: u32,
    pub physical_time_s: Option<f64>,
    pub compression_m: f64,
    pub thermal_strains: [f64; 2],
    pub pressure_pa: f64,
    pub gap_m: f64,
    pub model: &'static str,
}

impl ContactReferenceSpec {
    pub fn validate(&self) -> Result<()> {
        let h = self.size_m[2];
        let minimum = self.size_m.iter().copied().fold(f64::INFINITY, f64::min);
        let positive = |v: f64| v.is_finite() && v > 0.;
        if self.schema_version != 1
            || !self.synthetic
            || self.backend != "cpu"
            || self.formulation != "planar_linear_penalty_contact"
            || !(2..=16).contains(&self.resolution)
            || self.size_m.iter().any(|v| !(1e-6..=0.1).contains(v))
            || !self.geometry_tolerance_m.is_finite()
            || self.geometry_tolerance_m < 1e-10
            || self.geometry_tolerance_m >= 1e-3 * minimum
            || !(0. ..=0.01 * h).contains(&self.initial_gap_m)
            || [self.preload_compression_m, self.final_compression_m]
                .iter()
                .any(|v| !(0. ..=0.001 * h).contains(v))
            || self
                .young_modulus_pa
                .iter()
                .any(|v| !(1e4..=1e12).contains(v))
            || self
                .expansion_per_k
                .iter()
                .any(|v| !(0. ..=1e-4).contains(v))
            || !(100. ..=1000.).contains(&self.reference_temperature_k)
            || self
                .final_temperatures_k
                .iter()
                .any(|v| !(100. ..=1000.).contains(v))
            || !positive(self.contact_stiffness_pa_m)
            || self.contact_stiffness_pa_m > 1e16
            || !positive(self.compliance())
            || !positive(self.numerical_tolerance)
            || self.numerical_tolerance > 0.002
            || self
                .expansion_per_k
                .iter()
                .zip(self.final_temperatures_k)
                .any(|(a, t)| (a * (t - self.reference_temperature_k)).abs() > 0.001)
            || [
                &self.material_provenance,
                &self.contact_provenance,
                &self.boundary_provenance,
            ]
            .iter()
            .any(|v| v.trim().is_empty() || v.len() > 4096)
        {
            return Err(invalid(
                "explicit bounded synthetic zero-Poisson CPU planar contact, small strain, SI material/interface/boundary provenance and unchanged 0.002 gate required",
            ));
        }
        Ok(())
    }

    fn compliance(&self) -> f64 {
        self.young_modulus_pa
            .iter()
            .map(|e| self.size_m[2] / e)
            .sum::<f64>()
            + 1. / self.contact_stiffness_pa_m
    }

    pub fn reference(&self, state: u32) -> Result<ContactReferenceState> {
        self.validate()?;
        if !(1..=2).contains(&state) {
            return Err(invalid(
                "explicit preload or final static contact state required",
            ));
        }
        let compression_m = if state == 1 {
            self.preload_compression_m
        } else {
            self.final_compression_m
        };
        let thermal_strains = if state == 1 {
            [0.; 2]
        } else {
            std::array::from_fn(|i| {
                self.expansion_per_k[i]
                    * (self.final_temperatures_k[i] - self.reference_temperature_k)
            })
        };
        let closure = compression_m + self.size_m[2] * thermal_strains.iter().sum::<f64>()
            - self.initial_gap_m;
        let pressure_pa = closure.max(0.) / self.compliance();
        let gap_m = if pressure_pa > 0. {
            -pressure_pa / self.contact_stiffness_pa_m
        } else {
            -closure
        };
        Ok(ContactReferenceState {
            solver_step_parameter: state,
            physical_time_s: None,
            compression_m,
            thermal_strains,
            pressure_pa,
            gap_m,
            model: "p=max(0,compression+sum(alpha*dT*h)-gap)/(sum(h/E)+1/K); Poisson ratio zero",
        })
    }
}
