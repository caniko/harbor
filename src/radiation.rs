//! Explicit synthetic UV irradiance, optical weighting and prescribed exposure.
//! This prepares independent references; native transport execution is separate.
use crate::{
    Result,
    contracts::{digest, invalid},
    science::Quantity,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SpectralSource {
    Directional {
        propagation_direction: [f64; 3],
        irradiance: Vec<Quantity>,
    },
    Isotropic {
        radiance: Vec<Quantity>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RadiantHistoryPoint {
    pub time: Quantity,
    pub scale: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpectralReferenceSpec {
    pub schema_version: u32,
    pub synthetic: bool,
    pub backend: String,
    pub variant: String,
    pub precision: String,
    pub wavelengths: Vec<Quantity>,
    pub source: SpectralSource,
    pub source_provenance: String,
    pub sensor_width: Quantity,
    pub sensor_height: Quantity,
    pub sensor_normal: [f64; 3],
    pub occlusion: String,
    pub absorptivity: Vec<f64>,
    pub optical_provenance: String,
    pub ageing_action: Vec<f64>,
    pub ageing_provenance: String,
    pub history: Vec<RadiantHistoryPoint>,
    pub history_interpolation: String,
    pub history_provenance: String,
    pub samples: u32,
    pub seeds: [u32; 3],
    pub relative_tolerance: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NormalizedSpectralReference {
    pub wavelengths_m: Vec<f64>,
    /// Per metre spectral density: W/(m2*m) or W/(m2*sr*m).
    pub source_values_si: Vec<f64>,
    pub source_unit: String,
    pub sensor_size_m: [f64; 2],
    pub sensor_area_m2: f64,
    pub angular_reference_factor: f64,
    pub history_times_s: Vec<f64>,
    pub history_scales: Vec<f64>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreparedSpectralReference {
    pub schema_version: u32,
    pub preparation_id: String,
    pub input: SpectralReferenceSpec,
    pub normalized: NormalizedSpectralReference,
    pub incident_irradiance_w_m2: f64,
    pub absorbed_irradiance_w_m2: f64,
    pub ageing_weighted_irradiance_w_m2: f64,
    pub incident_power_w: f64,
    pub absorbed_power_w: f64,
    pub integrated_history_scale_s: f64,
    pub incident_exposure_j_m2: f64,
    pub absorbed_exposure_j_m2: f64,
    pub ageing_weighted_exposure_j_m2: f64,
    pub executed: bool,
    pub physical_validation: String,
    pub limitations: Vec<String>,
}
fn unit_vector(v: [f64; 3]) -> bool {
    v.into_iter().all(f64::is_finite)
        && (v.into_iter().map(|x| x * x).sum::<f64>() - 1.).abs() <= 1e-12
}
// Exact product integral for two piecewise-linear functions on one shared grid.
// Endpoint-product trapezoids are not exact when optical weights vary.
fn product_integral(x: &[f64], a: &[f64], b: &[f64]) -> f64 {
    x.windows(2)
        .enumerate()
        .map(|(i, x)| {
            let da = a[i + 1] - a[i];
            let db = b[i + 1] - b[i];
            (x[1] - x[0]) * (a[i] * b[i] + (a[i] * db + b[i] * da) / 2. + da * db / 3.)
        })
        .sum()
}
impl SpectralReferenceSpec {
    pub fn prepare(&self) -> Result<PreparedSpectralReference> {
        let n = self.wavelengths.len();
        if self.schema_version != 1
            || !self.synthetic
            || self.backend != "cpu"
            || self.variant != "scalar_spectral"
            || self.precision != "Float32"
            || !(2..=64).contains(&n)
            || self.absorptivity.len() != n
            || self.ageing_action.len() != n
            || self
                .absorptivity
                .iter()
                .chain(&self.ageing_action)
                .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
            || !unit_vector(self.sensor_normal)
            || !matches!(
                self.occlusion.as_str(),
                "none" | "full_directional_occluder"
            )
            || !(2..=32).contains(&self.history.len())
            || self.history_interpolation != "piecewise_linear_prescribed_scale"
            || self
                .history
                .iter()
                .any(|point| !point.scale.is_finite() || point.scale < 0. || point.scale > 1e6)
            || !self.samples.is_power_of_two()
            || !(1024..=65536).contains(&self.samples)
            || self.seeds[0] >= self.seeds[1]
            || self.seeds[1] >= self.seeds[2]
            || !self.relative_tolerance.is_finite()
            || self.relative_tolerance <= 0.
            || self.relative_tolerance > 0.02
            || [
                &self.source_provenance,
                &self.optical_provenance,
                &self.ageing_provenance,
                &self.history_provenance,
            ]
            .into_iter()
            .any(|p| p.trim().is_empty() || p.len() > 4096)
        {
            return Err(invalid(
                "bounded synthetic scalar Float32 spectral reference with explicit UV properties, angular source, complete prescribed history and unchanged acceptance required",
            ));
        }
        let wavelengths = self
            .wavelengths
            .iter()
            .map(|q| q.si("length"))
            .collect::<Result<Vec<_>>>()?;
        if wavelengths.iter().any(|v| !(200e-9..=2500e-9).contains(v))
            || wavelengths
                .windows(2)
                .any(|p| p[0] >= p[1] || (p[1] - p[0]) < 1e-12)
        {
            return Err(invalid(
                "strictly ordered explicitly sampled 200–2500 nm spectrum with no extrapolation required",
            ));
        }
        let size = [
            self.sensor_width.si("length")?,
            self.sensor_height.si("length")?,
        ];
        if size
            .into_iter()
            .any(|v| !v.is_finite() || !(1e-5..=0.01).contains(&v))
        {
            return Err(invalid(
                "finite bounded planar sensor dimensions in metres required",
            ));
        }
        let (values, dimension, unit, factor) = match &self.source {
            SpectralSource::Directional {
                propagation_direction,
                irradiance,
            } => {
                if !unit_vector(*propagation_direction) {
                    return Err(invalid(
                        "explicit unit propagation direction required; angular source cannot be inferred from scalar irradiance",
                    ));
                }
                let cosine = -propagation_direction
                    .iter()
                    .zip(self.sensor_normal)
                    .map(|(a, b)| a * b)
                    .sum::<f64>();
                (
                    irradiance,
                    "spectral_irradiance",
                    "W/(m2*m)",
                    if self.occlusion == "full_directional_occluder" {
                        0.
                    } else {
                        cosine.max(0.)
                    },
                )
            }
            SpectralSource::Isotropic { radiance } => {
                if self.occlusion != "none" {
                    return Err(invalid(
                        "directional full-cover screen cannot authorize hemispherical diffuse occlusion",
                    ));
                }
                (
                    radiance,
                    "spectral_radiance",
                    "W/(m2*sr*m)",
                    std::f64::consts::PI,
                )
            }
        };
        let values = values
            .iter()
            .map(|q| q.si(dimension))
            .collect::<Result<Vec<_>>>()?;
        if values.len() != n
            || values
                .iter()
                .any(|v| !v.is_finite() || *v < 0. || *v > 1e15)
            || values.iter().all(|v| *v == 0.)
        {
            return Err(invalid(
                "complete nonnegative finite spectral source and explicit spectral density units required",
            ));
        }
        let times = self
            .history
            .iter()
            .map(|p| p.time.si("time"))
            .collect::<Result<Vec<_>>>()?;
        if times[0] != 0.
            || times.windows(2).any(|p| p[0] >= p[1])
            || *times.last().unwrap() > 86400. * 365.
        {
            return Err(invalid(
                "complete strictly ordered prescribed exposure from t=0 with bounded duration required",
            ));
        }
        let scales = self.history.iter().map(|p| p.scale).collect::<Vec<_>>();
        let time_integral = times
            .windows(2)
            .enumerate()
            .map(|(i, p)| (p[1] - p[0]) * (scales[i] + scales[i + 1]) / 2.)
            .sum::<f64>();
        let incident = factor * product_integral(&wavelengths, &values, &vec![1.; n]);
        let absorbed = factor * product_integral(&wavelengths, &values, &self.absorptivity);
        let ageing = factor * product_integral(&wavelengths, &values, &self.ageing_action);
        let area = size[0] * size[1];
        if [
            incident,
            absorbed,
            ageing,
            time_integral,
            incident * time_integral,
            absorbed * time_integral,
            ageing * time_integral,
        ]
        .into_iter()
        .any(|v| !v.is_finite() || v < 0.)
        {
            return Err(invalid(
                "finite nonnegative spectral irradiance and radiant exposure integrals required",
            ));
        }
        let mut report=PreparedSpectralReference{schema_version:1,preparation_id:String::new(),input:self.clone(),
            normalized:NormalizedSpectralReference{wavelengths_m:wavelengths,source_values_si:values,source_unit:unit.into(),sensor_size_m:size,sensor_area_m2:area,
                angular_reference_factor:factor,history_times_s:times,history_scales:scales},
            incident_irradiance_w_m2:incident,absorbed_irradiance_w_m2:absorbed,ageing_weighted_irradiance_w_m2:ageing,
            incident_power_w:incident*area,absorbed_power_w:absorbed*area,integrated_history_scale_s:time_integral,
            incident_exposure_j_m2:incident*time_integral,absorbed_exposure_j_m2:absorbed*time_integral,ageing_weighted_exposure_j_m2:ageing*time_integral,
            executed:false,physical_validation:"unqualified".into(),limitations:vec![
                "synthetic fixed-angular source; no atmospheric derivation, sky-map reduction or imported CAD".into(),
                "piecewise-linear spectra and prescribed temporal amplitude; interpolation is the declared model, not validation of solar history sampling".into(),
                "absorbed heating, incident dose and dimensionless ageing-weighted exposure remain distinct; no temperature or ageing lifetime inference".into(),
                "Float32 scalar transport is a declared CPU reference; no CUDA, Vulkan/HIP or GPU transport qualification".into()]};
        report.preparation_id = digest(&report)?;
        Ok(report)
    }
}
