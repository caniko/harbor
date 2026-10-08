//! One-way identity/quadrature map from registered anisotropic atmospheric originals.
pub use crate::atmospheric_spectral_fields::{
    AtmosphericComponent, AtmosphericPacketChannels, reconstruct_native_packets,
};
use crate::{
    Error, Result,
    atmosphere::{self, AtmosphericReferenceSpec},
    contracts::{StageOperation, digest, invalid},
    qualification::{EvidenceRecord, EvidenceState},
    radiation::{SpectralReferenceSpec, SpectralSource, product_integral},
    storage::Store,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AtmosphericTransferRequest {
    pub schema_version: u32,
    pub source_job: String,
    /// The receiver's directional source is the explicit original TOA descriptor,
    /// not an invented ground spectrum. Original native attenuation is applied.
    pub receiver: SpectralReferenceSpec,
    pub angular_mapping: String,
    pub maximum_relative_conservation_error: f64,
}
impl AtmosphericTransferRequest {
    pub fn validate(&self) -> Result<()> {
        self.receiver.prepare()?;
        if self.schema_version != 1
            || uuid::Uuid::parse_str(&self.source_job).is_err()
            || self.angular_mapping != "native_midpoint_solid_angle_quadrature"
            || self.receiver.occlusion != "none"
            || !matches!(self.receiver.source, SpectralSource::Directional { .. })
            || !self.maximum_relative_conservation_error.is_finite()
            || self.maximum_relative_conservation_error <= 0.
            || self.maximum_relative_conservation_error > 1e-10
        {
            return Err(invalid(
                "strict original atmospheric source, unobstructed receiver, native angular midpoint identity and unchanged transfer gate required",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AtmosphericSurfaceReference {
    pub wavelengths_nm: Vec<f64>,
    pub direct_w_m2_nm: Vec<f64>,
    pub diffuse_w_m2_nm: Vec<f64>,
    pub incident_w_m2_nm: Vec<f64>,
    pub incident_irradiance_w_m2: f64,
    pub absorbed_irradiance_w_m2: f64,
    pub ageing_weighted_irradiance_w_m2: f64,
    pub sensor_area_m2: f64,
    pub incident_power_w: f64,
    pub absorbed_power_w: f64,
    pub integrated_history_scale_s: f64,
    pub incident_exposure_j_m2: f64,
    pub absorbed_exposure_j_m2: f64,
    pub ageing_weighted_exposure_j_m2: f64,
    pub angular_shape: [usize; 3],
    pub angular_cell_solid_angle_sr: f64,
    pub original_angular_flux_error: f64,
    pub original_angular_flux_tolerance: f64,
    pub transfer_relative_conservation_error: f64,
}

/// Native phi locates the receiver around the vertical (North=0, East=90).
/// Both upwelling and downwelling photons propagate toward that receiver.
pub fn propagation(umu: f64, phi_deg: f64) -> [f64; 3] {
    let radius = (1. - umu * umu).sqrt();
    let phi = phi_deg.to_radians();
    [radius * phi.sin(), radius * phi.cos(), umu]
}
fn cosine(normal: [f64; 3], direction: [f64; 3]) -> f64 {
    (-normal
        .into_iter()
        .zip(direction)
        .map(|(a, b)| a * b)
        .sum::<f64>())
    .max(0.)
}
fn sum(values: impl IntoIterator<Item = f64>) -> f64 {
    let (mut total, mut correction) = (0., 0.);
    for value in values {
        let adjusted = value - correction;
        let next = total + adjusted;
        correction = (next - total) - adjusted;
        total = next;
    }
    total
}

/// Compute an explicit angular-midpoint reference, not native surface transport.
/// Spectral products use the original knots and distinct absorption/ageing curves.
pub fn reference(
    source: &AtmosphericReferenceSpec,
    request: &AtmosphericTransferRequest,
    text: &str,
) -> Result<AtmosphericSurfaceReference> {
    request.validate()?;
    let prepared = source.prepare()?;
    let surface = request.receiver.prepare()?;
    let SpectralSource::Directional {
        propagation_direction,
        ..
    } = &request.receiver.source
    else {
        return Err(invalid(
            "explicit original atmospheric directional input required",
        ));
    };
    if propagation_direction
        .iter()
        .zip(prepared.propagation_direction)
        .any(|(a, b)| (a - b).abs() > 1e-12)
        || surface.normalized.wavelengths_m.len() != prepared.wavelengths_nm.len()
        || surface
            .normalized
            .wavelengths_m
            .iter()
            .zip(&prepared.wavelengths_nm)
            .any(|(a, b)| (a * 1e9 - b).abs() > 1e-9)
        || surface
            .normalized
            .source_values_si
            .iter()
            .zip(&prepared.toa_irradiance_w_m2_nm)
            .any(|(a, b)| (a * 1e-9 - b).abs() > 1e-12 * b.abs())
    {
        return Err(invalid(
            "receiver must bind unchanged original TOA spectral knots/units and propagation direction; ground attenuation comes only from native originals",
        ));
    }
    let observed = atmosphere::observations(source, text)?;
    let mut transfer_error = 0_f64;
    // Back-reconstruct every proposed native emitter value in the original
    // units. This is separate from the source angular quadrature gate and
    // from future Float32 transport/sampling error.
    for row in text.lines() {
        let values = row
            .split_whitespace()
            .map(str::parse::<f64>)
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|_| invalid("original atmospheric scalar required"))?;
        let vertical = -prepared.propagation_direction[2];
        for (original, mapped, measure) in
            std::iter::once((values[1], values[1] / vertical, vertical)).chain(
                values[4..].iter().map(|v| {
                    (
                        *v,
                        *v * prepared.angular_cell_solid_angle_sr,
                        1. / prepared.angular_cell_solid_angle_sr,
                    )
                }),
            )
        {
            let reconstructed = mapped * measure;
            let error = if original == 0. {
                if reconstructed == 0. {
                    0.
                } else {
                    f64::INFINITY
                }
            } else {
                (reconstructed / original - 1.).abs()
            };
            if !error.is_finite() {
                return Err(invalid(
                    "finite unchanged original source-to-emitter conservation required",
                ));
            }
            transfer_error = transfer_error.max(error);
        }
    }
    if transfer_error > request.maximum_relative_conservation_error {
        return Err(invalid(
            "native original source-to-emitter identity exceeds explicit conservation gate",
        ));
    }
    let normal = request.receiver.sensor_normal;
    let direct: Vec<_> = observed
        .direct_normal_w_m2_nm
        .iter()
        .map(|v| v * cosine(normal, prepared.propagation_direction))
        .collect();
    let angular: Vec<_> = prepared
        .umu
        .iter()
        .flat_map(|mu| {
            prepared.phi_deg.iter().map(move |phi| {
                cosine(normal, propagation(*mu, *phi)) * prepared.angular_cell_solid_angle_sr
            })
        })
        .collect();
    let diffuse: Vec<_> = text
        .lines()
        .map(|row| {
            sum(row
                .split_whitespace()
                .skip(4)
                .zip(&angular)
                .map(|(value, weight)| value.parse::<f64>().unwrap_or(f64::NAN) * weight))
        })
        .collect();
    let incident: Vec<_> = direct.iter().zip(&diffuse).map(|(a, b)| a + b).collect();
    let power = product_integral(
        &prepared.wavelengths_nm,
        &incident,
        &vec![1.; incident.len()],
    );
    let absorbed = product_integral(
        &prepared.wavelengths_nm,
        &incident,
        &request.receiver.absorptivity,
    );
    let ageing = product_integral(
        &prepared.wavelengths_nm,
        &incident,
        &request.receiver.ageing_action,
    );
    let area = surface.normalized.sensor_area_m2;
    let history = surface.integrated_history_scale_s;
    if incident
        .iter()
        .chain([
            &power,
            &absorbed,
            &ageing,
            &(power * area),
            &(absorbed * area),
            &(power * history),
            &(absorbed * history),
            &(ageing * history),
        ])
        .any(|v| !v.is_finite() || *v < 0.)
    {
        return Err(invalid(
            "finite nonnegative complete original angular irradiance, power and prescribed spectral dose required",
        ));
    }
    Ok(AtmosphericSurfaceReference {
        wavelengths_nm: prepared.wavelengths_nm,
        direct_w_m2_nm: direct,
        diffuse_w_m2_nm: diffuse,
        incident_w_m2_nm: incident,
        incident_irradiance_w_m2: power,
        absorbed_irradiance_w_m2: absorbed,
        ageing_weighted_irradiance_w_m2: ageing,
        sensor_area_m2: area,
        incident_power_w: power * area,
        absorbed_power_w: absorbed * area,
        integrated_history_scale_s: history,
        incident_exposure_j_m2: power * history,
        absorbed_exposure_j_m2: absorbed * history,
        ageing_weighted_exposure_j_m2: ageing * history,
        angular_shape: [
            source.wavelengths.len(),
            prepared.umu.len(),
            prepared.phi_deg.len(),
        ],
        angular_cell_solid_angle_sr: prepared.angular_cell_solid_angle_sr,
        original_angular_flux_error: observed.maximum_angular_flux_error,
        original_angular_flux_tolerance: observed.tolerance,
        transfer_relative_conservation_error: transfer_error,
    })
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AtmosphericTransferReport {
    pub schema_version: u32,
    pub transfer_id: String,
    pub request: AtmosphericTransferRequest,
    pub source_science_id: String,
    pub source_execution_id: String,
    pub source_execution_binding_digest: String,
    pub original_field: EvidenceRecord,
    pub original_receipt: EvidenceRecord,
    pub reference: AtmosphericSurfaceReference,
    pub source_association: String,
    pub destination_association: String,
    pub propagation_coordinates: String,
    pub interpolation: String,
    pub direct_mapping: String,
    pub diffuse_mapping: String,
    pub original_precision: String,
    pub reduction_precision: String,
    pub preserved_original_distribution: bool,
    pub transfer_relative_conservation_error: f64,
    pub executed: bool,
    pub native_transport: String,
    pub physical_validation: String,
    pub limitations: Vec<String>,
}
pub fn prepare(
    store: &Store,
    request: &AtmosphericTransferRequest,
) -> Result<AtmosphericTransferReport> {
    request.validate()?;
    let job = store.job(&request.source_job)?;
    let plan = store.recorded_plan(&request.source_job)?;
    let source = plan.atmosphere.as_ref().ok_or_else(|| {
        Error::Unqualified("registered native atmospheric source required".into())
    })?;
    let evidence = crate::qualification::inspect(store, &request.source_job)?;
    if job.state != "succeeded"
        || job.exit_code != Some(0)
        || !evidence.capabilities.iter().any(|cap| {
            cap.operation == StageOperation::AtmosphericReference
                && matches!(cap.runtime_execution, EvidenceState::Recorded)
                && matches!(cap.numerical_verification, EvidenceState::ReportedPass)
        })
    {
        return Err(Error::Unqualified(
            "complete succeeded execution-bound original angular atmospheric evidence required"
                .into(),
        ));
    }
    let (original_receipt, _) = crate::results::registered(
        store,
        &request.source_job,
        "stages/atmosphere/atmosphere-receipt.json",
    )?;
    let (original_field, bytes) = crate::results::registered_bytes(
        store,
        &request.source_job,
        atmosphere::ORIGINAL_PATH,
        "txt",
    )?;
    let surface = reference(
        source,
        request,
        std::str::from_utf8(&bytes)
            .map_err(|_| invalid("native original atmospheric UTF-8 required"))?,
    )?;
    let transfer_error = surface.transfer_relative_conservation_error;
    let mut report=AtmosphericTransferReport {schema_version:1,transfer_id:String::new(),request:request.clone(),source_science_id:plan.science_id()?,source_execution_id:job.plan_digest,
        source_execution_binding_digest:digest(&store.execution_binding(&request.source_job)?)?,original_field,original_receipt,reference:surface,
        source_association:atmosphere::FIELD_ASSOCIATION.into(),destination_association:"native_planar_surface_emitter_direction_sample".into(),propagation_coordinates:"East, North, Up; native phi sensor position North=0 East=90; umu is signed propagation vertical cosine".into(),
        interpolation:"identity at every original native wavelength and angular midpoint; exact piecewise-linear spectral-product integration".into(),
        direct_mapping:"native direct normal irradiance = original edir / cos(sza); unchanged explicit solar propagation direction".into(),
        diffuse_mapping:"each original uu cell becomes normal irradiance uu*dOmega at its original photon propagation midpoint; no isotropic collapse, rescaling, clipping or RGB upsampling".into(),
        original_precision:"Float32; authoritative unchanged native text".into(),reduction_precision:"Float64".into(),preserved_original_distribution:true,
        transfer_relative_conservation_error:transfer_error,executed:false,native_transport:"not_executed".into(),physical_validation:"unqualified".into(),limitations:vec![
            "reference is the original angular midpoint quadrature; no continuous angular interpolation, extra knots or extrapolation".into(),
            "direct and diffuse fields remain independent; source stream/angular/spectral convergence must be assessed separately".into(),
            "receiver directional source binds original TOA inputs only; actual ground source uses retained native attenuation and angular radiance".into(),
            "unobstructed planar receiver and fixed-angular prescribed amplitude history only; no transport, weather-history, heating/lifetime or physical qualification".into()]};
    report.transfer_id = digest(&report)?;
    Ok(report)
}
