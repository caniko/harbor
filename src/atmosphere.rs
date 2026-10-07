//! Explicit pinned molecular UV atmosphere and retained native angular samples.
use crate::{
    Result,
    contracts::{digest, invalid},
    science::Quantity,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const SOURCE_SHA256: &str = "64930cc40b6e4a37aa220520974d330fc1563796f466a649b2238131f2d69840";
pub const SANDBOX_POLICY: &str = "harbor-cad-atmosphere-cpu-v1";

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AtmosphericReferenceSpec {
    pub schema_version: u32,
    pub synthetic: bool,
    pub backend: String,
    pub solver: String,
    pub model: String,
    pub profile: String,
    pub wavelengths: Vec<Quantity>,
    pub toa_irradiance: Vec<Quantity>,
    pub solar_zenith_deg: f64,
    pub solar_azimuth_deg: f64,
    pub albedo: f64,
    pub streams: u32,
    pub mu_bins: u32,
    pub phi_bins: u32,
    pub relative_tolerance: f64,
    pub source_provenance: String,
    pub atmosphere_provenance: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreparedAtmosphericReference {
    pub schema_version: u32,
    pub executed: bool,
    pub request_sha256: String,
    pub source_sha256: String,
    pub wavelengths_nm: Vec<f64>,
    pub toa_irradiance_w_m2_nm: Vec<f64>,
    /// Propagation in explicit East, North, Up coordinates, not a view direction.
    pub propagation_direction: [f64; 3],
    pub umu: Vec<f64>,
    pub phi_deg: Vec<f64>,
    pub angular_cell_solid_angle_sr: f64,
    pub transparent_horizontal_reference_w_m2_nm: Option<Vec<f64>>,
    pub physical_validation: String,
}

impl AtmosphericReferenceSpec {
    pub fn prepare(&self) -> Result<PreparedAtmosphericReference> {
        let bins = [8, 16, 32, 64];
        if self.schema_version != 1
            || !self.synthetic
            || self.backend != "cpu"
            || self.solver != "disort"
            || !["clear_sky_molecular_crs", "transparent_reference"].contains(&self.model.as_str())
            || !["afglms", "afglmw"].contains(&self.profile.as_str())
            || !bins.contains(&self.streams)
            || !bins.contains(&self.mu_bins)
            || !bins.contains(&self.phi_bins)
            || !self.solar_zenith_deg.is_finite()
            || !(0. ..=80.).contains(&self.solar_zenith_deg)
            || !self.solar_azimuth_deg.is_finite()
            || !(0. ..360.).contains(&self.solar_azimuth_deg)
            || !self.albedo.is_finite()
            || !(0. ..=1.).contains(&self.albedo)
            || (self.model == "transparent_reference" && self.albedo != 0.)
            || !self.relative_tolerance.is_finite()
            || self.relative_tolerance <= 0.
            || self.relative_tolerance > 0.02
            || !(2..=64).contains(&self.wavelengths.len())
            || self.toa_irradiance.len() != self.wavelengths.len()
            || [&self.source_provenance, &self.atmosphere_provenance]
                .iter()
                .any(|s| s.trim().is_empty() || s.len() > 2048)
        {
            return Err(invalid(
                "explicit bounded synthetic DISORT molecular UV/transparent model, atmospheric/solar provenance and unchanged angular/analytic gate required",
            ));
        }
        let wavelengths_nm = self
            .wavelengths
            .iter()
            .map(|q| Ok(q.si("length")? * 1e9))
            .collect::<Result<Vec<_>>>()?;
        if wavelengths_nm.iter().any(|w| {
            *w < 280. - 1e-12 || *w > 400. + 1e-12 || (w * 1000. - (w * 1000.).round()).abs() > 1e-6
        }) || wavelengths_nm
            .windows(2)
            .any(|w| (w[0] * 1000.).round() >= (w[1] * 1000.).round())
        {
            return Err(invalid(
                "2–64 ordered original UV knots in supported molecular-cross-section band 280–400 nm on the native 0.001 nm decimal-output grid required",
            ));
        }
        let toa = self
            .toa_irradiance
            .iter()
            .map(|q| Ok(q.si("spectral_irradiance")? * 1e-9))
            .collect::<Result<Vec<_>>>()?;
        if toa.iter().any(|v| *v < 0. || *v > 1e6) || !toa.iter().any(|v| *v > 0.) {
            return Err(invalid(
                "explicit finite nonnegative top-of-atmosphere irradiance with a nonzero UV knot required",
            ));
        }
        let zenith = self.solar_zenith_deg.to_radians();
        let azimuth = self.solar_azimuth_deg.to_radians();
        let umu = (0..2 * self.mu_bins)
            .map(|i| -1. + (f64::from(i) + 0.5) / f64::from(self.mu_bins))
            .collect();
        let phi_deg = (0..self.phi_bins)
            .map(|i| (f64::from(i) + 0.5) * 360. / f64::from(self.phi_bins))
            .collect();
        let reference = (self.model == "transparent_reference")
            .then(|| toa.iter().map(|e| e * zenith.cos()).collect());
        Ok(PreparedAtmosphericReference {
            schema_version: 1,
            executed: false,
            request_sha256: digest(self)?,
            source_sha256: SOURCE_SHA256.into(),
            wavelengths_nm,
            toa_irradiance_w_m2_nm: toa,
            propagation_direction: [
                zenith.sin() * azimuth.sin(),
                zenith.sin() * azimuth.cos(),
                -zenith.cos(),
            ],
            umu,
            phi_deg,
            angular_cell_solid_angle_sr: 2. * std::f64::consts::PI
                / (f64::from(self.mu_bins) * f64::from(self.phi_bins)),
            transparent_horizontal_reference_w_m2_nm: reference,
            physical_validation: "unqualified".into(),
        })
    }
}
