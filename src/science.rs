use crate::{Result, contracts::invalid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Quantity {
    pub value: f64,
    pub unit: String,
}
impl Quantity {
    pub fn si(&self, dimension: &str) -> Result<f64> {
        if !self.value.is_finite() {
            return Err(invalid("non-finite quantity"));
        }
        let (factor, offset) = match (dimension, self.unit.as_str()) {
            ("length", "m")
            | ("temperature", "K")
            | ("density", "kg/m3")
            | ("kinematic_viscosity", "m2/s")
            | ("acceleration", "m/s2") => (1., 0.),
            ("length", "mm") => (0.001, 0.),
            ("temperature", "degC") => (1., 273.15),
            _ => {
                return Err(invalid(format!(
                    "unsupported unit {} for {dimension}",
                    self.unit
                )));
            }
        };
        Ok(self.value * factor + offset)
    }
}
// Magnus screening over water, 0–50 °C. This is not a moisture solver.
pub fn dew_point(temperature_k: f64, rh: f64) -> Result<f64> {
    let t = temperature_k - 273.15;
    if !t.is_finite() || !(0.0..=50.).contains(&t) || !rh.is_finite() || rh <= 0. || rh > 1. {
        return Err(invalid(
            "dew point screening requires 0–50 °C and 0 < RH <= 1",
        ));
    }
    let gamma = rh.ln() + 17.625 * t / (243.04 + t);
    Ok(243.04 * gamma / (17.625 - gamma) + 273.15)
}
#[derive(Debug, Serialize, Deserialize)]
pub struct ChannelReference {
    pub process: String,
    pub convergence: String,
    pub physical_validation: String,
    pub numerical_error: f64,
    pub mean_velocity_m_s: f64,
    pub y_m: Vec<f64>,
    pub velocity_m_s: Vec<f64>,
}
// Independent closed-form reference, not a substitute for OpenLB execution.
pub fn channel_reference(
    acceleration: f64,
    height: f64,
    viscosity: f64,
    points: usize,
) -> Result<ChannelReference> {
    if !acceleration.is_finite()
        || !height.is_finite()
        || !viscosity.is_finite()
        || height <= 0.
        || viscosity <= 0.
        || !(3..=1_000_000).contains(&points)
    {
        return Err(invalid("bounded channel reference inputs"));
    }
    let y_m: Vec<_> = (0..points)
        .map(|i| height * i as f64 / (points - 1) as f64)
        .collect();
    let velocity_m_s: Vec<_> = y_m
        .iter()
        .map(|y| acceleration * y * (height - y) / (2. * viscosity))
        .collect();
    let h = height / (points - 1) as f64;
    let mut numerical_error: f64 = 0.;
    for v in velocity_m_s.windows(3) {
        let residual = viscosity * (v[0] - 2. * v[1] + v[2]) / (h * h) + acceleration;
        numerical_error = numerical_error.max(residual.abs());
    }
    Ok(ChannelReference {
        process: "succeeded".into(),
        convergence: "analytical".into(),
        physical_validation: "unqualified".into(),
        numerical_error,
        mean_velocity_m_s: acceleration * height * height / (12. * viscosity),
        y_m,
        velocity_m_s,
    })
}
