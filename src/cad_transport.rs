//! Independent direct-only optical approval inputs for original material-tagged CAD.
use crate::{
    Result,
    cad_spectral::{CadSpectralSceneRequest, PreparedCadSpectralScene},
    contracts::{digest, invalid},
    radiation::{
        PreparedSpectralReference, RadiantHistoryPoint, SpectralReferenceSpec, SpectralSource,
    },
    science::Quantity,
    storage::Store,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const SANDBOX_POLICY: &str = "harbor-cad-cad-spectral-direct-cpu-v1";

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CadSpectralTransportRequest {
    pub schema_version: u32,
    pub scene: CadSpectralSceneRequest,
    pub formulation: String,
    pub source: SpectralSource,
    pub source_provenance: String,
    pub history: Vec<RadiantHistoryPoint>,
    pub history_interpolation: String,
    pub history_provenance: String,
    pub samples_per_triangle: u32,
    pub seeds: [u32; 3],
    pub relative_tolerance: f64,
    pub maximum_geometry_rounding_error_m: f64,
}

impl CadSpectralTransportRequest {
    /// Share the original source-unit and history quadrature contract. The
    /// auxiliary reference sensor is solely validation; it supplies no CAD area,
    /// normal, material, visibility, optical result or physical qualification.
    pub fn illumination(&self) -> Result<PreparedSpectralReference> {
        self.scene.validate()?;
        if self.schema_version != 1
            || self.formulation != "opaque_lambertian_direct_only"
            || !matches!(self.source, SpectralSource::Directional { .. })
            || !self.samples_per_triangle.is_power_of_two()
            || !(64..=4096).contains(&self.samples_per_triangle)
            || !self.maximum_geometry_rounding_error_m.is_finite()
            || self.maximum_geometry_rounding_error_m <= 0.
            || self.maximum_geometry_rounding_error_m > 1e-8
            || self.maximum_geometry_rounding_error_m
                > self.scene.geometry_tolerance.si("length")?
        {
            return Err(invalid(
                "explicit direct-only collimated optical source, bounded per-facet samples and unchanged native rounding budget required",
            ));
        }
        SpectralReferenceSpec {
            schema_version: 1,
            synthetic: true,
            backend: "cpu".into(),
            variant: "scalar_spectral".into(),
            precision: "Float32".into(),
            wavelengths: self.scene.wavelengths.clone(),
            source: self.source.clone(),
            source_provenance: self.source_provenance.clone(),
            sensor_width: Quantity {
                value: 1.,
                unit: "mm".into(),
            },
            sensor_height: Quantity {
                value: 1.,
                unit: "mm".into(),
            },
            sensor_normal: [0., 0., 1.],
            occlusion: "none".into(),
            absorptivity: vec![1.; self.scene.wavelengths.len()],
            optical_provenance:
                "unit numerical normalization; no inferred original region material".into(),
            ageing_action: vec![1.; self.scene.wavelengths.len()],
            ageing_provenance: "unit numerical normalization; no inferred calibrated ageing curve"
                .into(),
            history: self.history.clone(),
            history_interpolation: self.history_interpolation.clone(),
            history_provenance: self.history_provenance.clone(),
            samples: 1024,
            seeds: self.seeds,
            relative_tolerance: self.relative_tolerance,
        }
        .prepare()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CadSpectralTransportSpec {
    pub schema_version: u32,
    pub request: CadSpectralTransportRequest,
    pub scene: PreparedCadSpectralScene,
}

impl CadSpectralTransportSpec {
    pub fn validate(&self) -> Result<()> {
        self.request.illumination()?;
        let missing = self.request.scene.validate()?;
        let mut scene = self.scene.clone();
        scene.scene_id.clear();
        if self.schema_version != 1
            || self.scene.schema_version != 1
            || digest(&scene)? != self.scene.scene_id
            || digest(&self.scene.request)? != digest(&self.request.scene)?
            || self.scene.executed
            || self.scene.physical_validation != "unqualified"
            || self.scene.transport_readiness != "prepared_not_executed"
            || missing.iter().any(|s| s.ends_with(".response"))
            || missing != self.scene.missing_inputs
            || self.scene.ageing_readiness
                != if missing.is_empty() {
                    "prepared_not_executed"
                } else {
                    "missing_inputs"
                }
            || self.scene.regions.len() != self.request.scene.assignments.len()
            || self.scene.regions.is_empty()
        {
            return Err(invalid(
                "unchanged complete original material scene, explicit known optics and independent preparation identity required; missing ageing remains unknown",
            ));
        }
        let mut names = std::collections::BTreeSet::new();
        let mut facets = 0u32;
        let synthetic = self.scene.regions[0].source.geometry.synthetic;
        for region in &self.scene.regions {
            region.source.validate()?;
            let geometry = &region.source.geometry;
            let original = &region.original_triangles;
            if !names.insert(&region.assignment.region_name)
                || !self
                    .request
                    .scene
                    .assignments
                    .iter()
                    .any(|a| digest(a).ok() == digest(&region.assignment).ok())
                || region.assignment.region_name != geometry.region_name
                || region.source.job_id != self.request.scene.source_job
                || original.schema_version != 1
                || original.format != "stl"
                || original.path != format!("{}.stl", geometry.region_name)
                || original.sha256.len() != 64
                || !original
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                || !(12..=192).contains(&region.geometry.triangles)
                || original.bytes != 84 + 50 * region.geometry.triangles as u64
                || geometry.synthetic != synthetic
                || self.request.scene.geometry_tolerance.si("length")?
                    > geometry.geometry_tolerance_m
                || self.request.maximum_geometry_rounding_error_m > geometry.geometry_tolerance_m
            {
                return Err(invalid(
                    "complete unique original closed STL identity and named source context required",
                ));
            }
            facets += region.geometry.triangles as u32;
        }
        if facets * self.request.samples_per_triangle > 65536 {
            return Err(invalid(
                "aggregate complete original-facet sample budget exhausted",
            ));
        }
        for (i, a) in self.scene.regions.iter().enumerate() {
            for b in &self.scene.regions[i + 1..] {
                let a = a.source.geometry.bounds_m;
                let b = b.source.geometry.bounds_m;
                if !(0..3)
                    .any(|axis| a[2 * axis + 1] < b[2 * axis] || b[2 * axis + 1] < a[2 * axis])
                {
                    return Err(invalid(
                        "separated disjoint opaque original boxes required; no touching or overlapping region inference",
                    ));
                }
            }
        }
        Ok(())
    }
    pub fn native_request(&self) -> Result<serde_json::Value> {
        self.validate()?;
        let request = &self.request;
        Ok(
            serde_json::json!({"schema_version":1,"synthetic":self.scene.regions[0].source.geometry.synthetic,
            "backend":"cpu","variant":"scalar_spectral","precision":"Float32","formulation":request.formulation,
            "scene":self.scene,"source":request.source,"source_provenance":request.source_provenance,
            "history":request.history,"history_interpolation":request.history_interpolation,"history_provenance":request.history_provenance,
            "samples_per_triangle":request.samples_per_triangle,"seeds":request.seeds,"relative_tolerance":request.relative_tolerance,
            "maximum_geometry_rounding_error_m":request.maximum_geometry_rounding_error_m}),
        )
    }
}

pub fn resolve(
    store: &Store,
    request: CadSpectralTransportRequest,
) -> Result<CadSpectralTransportSpec> {
    request.illumination()?;
    let scene = crate::cad_spectral::prepare(store, request.scene.clone())?;
    let spec = CadSpectralTransportSpec {
        schema_version: 1,
        request,
        scene,
    };
    spec.validate()?;
    Ok(spec)
}
