//! Bounded original-region optical power/dose views at one explicitly retained seed.
use crate::{
    Error, Result,
    cad_transport::{RECEIPT_PATH, STAGE},
    contracts::{digest, invalid, token},
    qualification::{self, EvidenceRecord, EvidenceState},
    storage::Store,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CadOpticalResultsRequest {
    pub schema_version: u32,
    pub job_id: String,
    pub seed: u32,
    pub region_name: String,
}
impl CadOpticalResultsRequest {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1
            || uuid::Uuid::parse_str(&self.job_id).is_err()
            || !token(&self.region_name)
            || self.region_name.len() > 64
            || self.seed == 0
        {
            return Err(invalid(
                "versioned original CAD optical job, explicit seed and exact named region required",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CadOpticalResultsReport {
    pub schema_version: u32,
    pub request: CadOpticalResultsRequest,
    pub science_id: String,
    pub execution_id: String,
    pub execution_binding_digest: String,
    pub source_job_id: String,
    pub source_execution_id: String,
    pub material_name: String,
    pub material: crate::cad_spectral::CadSpectralMaterial,
    pub geometry_synthetic: bool,
    pub illumination: crate::radiation::SpectralSource,
    pub illumination_provenance: String,
    pub wavelengths: Vec<crate::science::Quantity>,
    pub source_triangles: EvidenceRecord,
    pub original_packets: EvidenceRecord,
    pub stage_receipt: EvidenceRecord,
    pub original_area_m2: f64,
    pub field_units: BTreeMap<String, String>,
    pub facets: Vec<Value>,
    pub physical_time_s: Option<f64>,
    pub history: Vec<crate::radiation::RadiantHistoryPoint>,
    pub history_provenance: String,
    pub association: String,
    pub interpolation: String,
    pub sampling_convergence: String,
    pub physical_validation: String,
}

pub fn inspect(
    store: &Store,
    request: CadOpticalResultsRequest,
) -> Result<CadOpticalResultsReport> {
    request.validate()?;
    let job = store.job(&request.job_id)?;
    let plan = store.recorded_plan(&request.job_id)?;
    let spec = plan.cad_transport.as_ref().ok_or_else(|| {
        Error::Unqualified(
            "independently approved direct-only original-CAD optical job required".into(),
        )
    })?;
    spec.validate_plan(&plan)?;
    if !spec.request.seeds.contains(&request.seed) {
        return Err(invalid(
            "explicit retained native optical seed required; no seed interpolation or averaging",
        ));
    }
    let region = spec
        .scene
        .regions
        .iter()
        .find(|r| r.assignment.region_name == request.region_name)
        .ok_or_else(|| invalid("exact original complete material region required"))?;
    let qualification = qualification::inspect(store, &job.id)?;
    if job.state != "succeeded"
        || job.exit_code != Some(0)
        || !qualification.capabilities.iter().any(|c| {
            c.stage_id == STAGE
                && matches!(c.runtime_execution, EvidenceState::Recorded)
                && matches!(c.numerical_verification, EvidenceState::ReportedPass)
        })
    {
        return Err(Error::Unqualified("independent native execution, sandbox and original optical packet numerical evidence required".into()));
    }
    let (receipt_record, receipt) =
        qualification::registered_json(store, &job.id, RECEIPT_PATH)?
            .ok_or_else(|| invalid("registered original optical receipt required"))?;
    let observation = receipt["observations"]
        .as_array()
        .and_then(|o| o.iter().find(|o| o["seed"] == request.seed))
        .ok_or_else(|| invalid("complete original optical seed observation required"))?;
    let facets = observation["facets"]
        .as_array()
        .ok_or_else(|| invalid("complete original facet scientific reductions required"))?
        .iter()
        .filter(|f| f["region"] == request.region_name)
        .cloned()
        .collect::<Vec<_>>();
    if facets.len() != region.geometry.triangles || facets.len() > 24 {
        return Err(invalid(
            "bounded complete original-region facet results required",
        ));
    }
    let original_packets = store
        .artifact_record(
            &job.id,
            &format!("stages/{STAGE}/triangles-{}.csv", request.seed),
        )?
        .ok_or_else(|| invalid("registered complete original optical packets required"))?;
    let source_triangles = store
        .artifact_record(
            &job.id,
            &format!("source-cad/{}", region.original_triangles.path),
        )?
        .ok_or_else(|| invalid("registered complete original CAD triangles required"))?;
    let evidence = |record: crate::contracts::ArtifactManifest| EvidenceRecord {
        path: record.path,
        sha256: record.sha256,
        bytes: record.bytes,
    };
    Ok(CadOpticalResultsReport {
        schema_version: 1,
        science_id: plan.science_id()?,
        execution_id: plan.id()?,
        execution_binding_digest: digest(&store.execution_binding(&job.id)?)?,
        source_job_id: spec.request.scene.source_job.clone(),
        source_execution_id: region.source.execution_id.clone(),
        material_name: region.assignment.material_name.clone(),
        material: spec
            .request
            .scene
            .materials
            .iter()
            .find(|m| m.name == region.assignment.material_name)
            .ok_or_else(|| invalid("approved complete optical material required"))?
            .clone(),
        geometry_synthetic: region.source.geometry.synthetic,
        illumination: spec.request.source.clone(),
        illumination_provenance: spec.request.source_provenance.clone(),
        wavelengths: spec.request.scene.wavelengths.clone(),
        source_triangles: evidence(source_triangles),
        original_packets: evidence(original_packets),
        stage_receipt: receipt_record,
        original_area_m2: region.geometry.area_m2,
        field_units: BTreeMap::from([
            ("original_area_m2".into(), "m2".into()),
            (
                "mean_spectral_irradiance_w_m2_nm".into(),
                "W/(m2*nm)".into(),
            ),
            ("channels_w_m2".into(), "W/m2".into()),
            ("reference_channels_w_m2".into(), "W/m2".into()),
            ("relative_errors".into(), "1".into()),
            ("power_w".into(), "W".into()),
            ("dose_j_m2".into(), "J/m2".into()),
            ("energy_j".into(), "J".into()),
        ]),
        facets,
        physical_time_s: None,
        history: spec.request.history.clone(),
        history_provenance: spec.request.history_provenance.clone(),
        association: "original_facet_order_with_explicit_whole_region_material".into(),
        interpolation: "none_original_declared_seed_and_complete_facets".into(),
        sampling_convergence: "not_assessed".into(),
        physical_validation: "unqualified".into(),
        request,
    })
}
