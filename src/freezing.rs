//! Input and SI conversion for the pinned conduction-induced Stefan reference.
//! Native OpenLB owns the total-enthalpy collision and phase-change coupling.
use crate::{Result, contracts::invalid, moisture_results::NativeMoistureAssessment};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreezingReferenceSpec {
    pub schema_version: u32,
    pub synthetic: bool,
    pub backend: String,
    pub formulation: String,
    /// Native x/y dimensions and explicitly prescribed out-of-plane extrusion.
    pub size_m: [f64; 3],
    pub resolution: u32,
    /// Equal constant properties in solid and liquid; no expansion or flow.
    pub density_kg_m3: f64,
    pub specific_heat_j_kg_k: f64,
    pub conductivity_w_m_k: f64,
    pub latent_heat_j_kg: f64,
    pub melting_temperature_k: f64,
    pub initial_temperature_k: f64,
    pub cold_wall_temperature_k: f64,
    pub material_temperature_domain_k: [f64; 2],
    pub steps: u64,
    pub observation_steps: Vec<u64>,
    pub front_tolerance: f64,
    pub temperature_tolerance: f64,
    pub mass_tolerance: f64,
    pub energy_tolerance: f64,
    pub material_provenance: String,
    pub boundary_provenance: String,
    pub geometry_provenance: String,
    pub moisture_risk: NativeMoistureAssessment,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct FreezingScale {
    pub spacing_m: f64,
    pub physical_step_s: f64,
    pub duration_s: f64,
    pub stefan_number: f64,
    /// h = cp*(T-Tcold) + L*f_liquid. This is a declared energy zero.
    pub initial_specific_enthalpy_j_kg: f64,
}

impl FreezingReferenceSpec {
    pub fn inspect(&self) -> Result<serde_json::Value> {
        let scale = self.scale()?;
        Ok(serde_json::json!({"valid":true,"executed":false,
            "science_id":crate::contracts::digest(self)?,"scale":scale,
            "moisture_risk":self.moisture_risk.inspect(self.cold_wall_temperature_k)?,
            "physical_validation":"unqualified",
            "scope":"synthetic fixed-volume conduction-induced solidification reference; retained-water transfer and native execution require separate qualification",
            "native_scaling":{"temperature":"(T-Tcold)/(Tm-Tcold)",
                "specific_enthalpy":"h/(cp*(Tm-Tcold)); energy zero at solid Tcold",
                "heat_descriptor":"D2Q5","relaxation_time":1.0,"diffusivity_lattice":1.0/6.0},
            "boundaries":{"xmin":"prescribed constant cold temperature","xmax":"insulated",
                "y":"periodic","velocity":"zero; conduction only","out_of_plane":"prescribed extrusion"}}))
    }

    pub fn scale(&self) -> Result<FreezingScale> {
        let positive = |v: f64| v.is_finite() && v > 0.;
        let n = u64::from(self.resolution);
        let span = self.melting_temperature_k - self.cold_wall_temperature_k;
        let domain = self.material_temperature_domain_k;
        if self.schema_version != 1
            || !self.synthetic
            || self.backend != "cpu"
            || self.formulation != "conduction_stefan_solidification_2d"
            || !(32..=128).contains(&self.resolution)
            || !self.resolution.is_multiple_of(8)
            || self.size_m.iter().any(|v| !(1e-6..=1.).contains(v))
            || self.size_m[1] != self.size_m[0] / 8.
            || ![
                self.density_kg_m3,
                self.specific_heat_j_kg_k,
                self.conductivity_w_m_k,
                self.latent_heat_j_kg,
            ]
            .into_iter()
            .all(positive)
            || ![
                self.melting_temperature_k,
                self.initial_temperature_k,
                self.cold_wall_temperature_k,
            ]
            .into_iter()
            .all(|v| (100. ..=1000.).contains(&v))
            || !positive(span)
            || self.initial_temperature_k != self.melting_temperature_k
            || !domain.into_iter().all(|v| (100. ..=1000.).contains(&v))
            || domain[0] >= domain[1]
            || self.cold_wall_temperature_k < domain[0]
            || self.melting_temperature_k > domain[1]
            || self.steps < n * n / 2
            || self.steps > n * n
            || self.observation_steps.len() < 2
            || self.observation_steps.len() > 16
            || self.observation_steps.first() != Some(&0)
            || self.observation_steps.last() != Some(&self.steps)
            || self.observation_steps.windows(2).any(|v| v[0] >= v[1])
            || self
                .observation_steps
                .iter()
                .any(|v| *v != 0 && (*v < n * n / 2 || *v > self.steps))
            || [self.front_tolerance, self.temperature_tolerance]
                .into_iter()
                .any(|v| !positive(v) || v > 0.02)
            || [self.mass_tolerance, self.energy_tolerance]
                .into_iter()
                .any(|v| !positive(v) || v > 1e-10)
            || [
                &self.material_provenance,
                &self.boundary_provenance,
                &self.geometry_provenance,
            ]
            .into_iter()
            .any(|v| v.trim().is_empty() || v.len() > 4096)
        {
            return Err(invalid(
                "explicit bounded synthetic equal-property conduction solidification inputs, material domain and unchanged gates required",
            ));
        }
        self.moisture_risk.inspect(self.cold_wall_temperature_k)?;
        let stefan = self.specific_heat_j_kg_k * span / self.latent_heat_j_kg;
        let spacing = self.size_m[0] / f64::from(self.resolution);
        let diffusivity =
            self.conductivity_w_m_k / (self.density_kg_m3 * self.specific_heat_j_kg_k);
        // D2Q5 cs²=1/3, fixed tau=1 => alpha_lattice=1/6. No physical
        // property is changed to hold tau constant while refining the grid.
        let step = spacing * spacing / (6. * diffusivity);
        let duration = self.steps as f64 * step;
        let enthalpy = self.specific_heat_j_kg_k * span + self.latent_heat_j_kg;
        if !(0.05..=0.2).contains(&stefan)
            || ![spacing, diffusivity, step, duration, enthalpy]
                .into_iter()
                .all(positive)
        {
            return Err(invalid(
                "finite SI conversion and Stefan number 0.05..0.2 required for the reference",
            ));
        }
        Ok(FreezingScale {
            spacing_m: spacing,
            physical_step_s: step,
            duration_s: duration,
            stefan_number: stefan,
            initial_specific_enthalpy_j_kg: enthalpy,
        })
    }
}
