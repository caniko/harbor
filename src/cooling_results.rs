//! Exact retained-time original cooling values with complete source provenance.
use crate::{Result, contracts::invalid, storage::Store};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CoolingSampleRequest {
    pub job_id: String,
    pub physical_time_s: f64,
    pub region: String,
    /// Exact native child indices, not interpolated positions.
    pub points: Vec<[usize; 2]>,
}

impl CoolingSampleRequest {
    pub fn validate(&self) -> Result<()> {
        if uuid::Uuid::parse_str(&self.job_id).is_err()
            || !self.physical_time_s.is_finite()
            || self.physical_time_s < 0.
            || !crate::contracts::token(&self.region)
            || self.region.len() > 64
            || !(1..=64).contains(&self.points.len())
            || self
                .points
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.points.len()
        {
            return Err(invalid(
                "one exact original cooling job, region, retained time and 1..64 distinct native subcontrols required",
            ));
        }
        Ok(())
    }
}

pub fn sample(store: &Store, request: &CoolingSampleRequest) -> Result<serde_json::Value> {
    request.validate()?;
    let plan = store.recorded_plan(&request.job_id)?;
    let spec = plan
        .retained_cooling
        .as_ref()
        .ok_or_else(|| invalid("registered source-bound cooling job required"))?;
    if request.region != spec.request.initialization.retained.destination_region {
        return Err(invalid(
            "exact original destination cooling region required",
        ));
    }
    let index = spec
        .times_s()
        .iter()
        .position(|t| *t == request.physical_time_s)
        .ok_or_else(|| {
            invalid(
                "exact retained physical cooling time required; no interpolation or extrapolation",
            )
        })?;
    let job = store.job(&request.job_id)?;
    let qualification = crate::qualification::inspect(store, &request.job_id)?;
    if job.state != "succeeded"
        || job.exit_code != Some(0)
        || !qualification.capabilities.iter().any(|c| {
            c.stage_id == crate::cooling_execution::STAGE
                && matches!(
                    c.runtime_execution,
                    crate::qualification::EvidenceState::Recorded
                )
                && matches!(
                    c.numerical_verification,
                    crate::qualification::EvidenceState::ReportedPass
                )
        })
    {
        return Err(crate::Error::Unqualified(
            "succeeded execution-bound original cooling evidence required".into(),
        ));
    }
    let (receipt_record, receipt) = crate::results::registered(
        store,
        &request.job_id,
        crate::cooling_execution::RECEIPT_PATH,
    )?;
    crate::cooling_fields::registered(store, &request.job_id, &plan, &receipt)?;
    let source = crate::cooling_execution::registered(store, &request.job_id, &plan)?;
    let original = crate::worker::read_bounded(&source, 16 * 1024 * 1024)?;
    if format!("{:x}", Sha256::digest(&original)) != spec.prepared.retained.original_field.sha256 {
        return Err(invalid(
            "original retained water source changed during cooling query",
        ));
    }
    let step = spec.request.observation_steps()[index];
    let path = format!("stages/retained-cooling/cooling-{step}.csv");
    let record = store
        .artifact_record(&request.job_id, &path)?
        .ok_or_else(|| invalid("registered cooling observation required"))?;
    let raw = crate::worker::read_bounded(
        &crate::storage::safe_path(&store.job_dir(&request.job_id)?, &path)?,
        64 * 1024 * 1024,
    )?;
    if raw.len() as u64 != record.bytes || format!("{:x}", Sha256::digest(&raw)) != record.sha256 {
        return Err(invalid(
            "registered original cooling values changed during query",
        ));
    }
    let nodes = crate::cooling_fields::nodes(spec, &original, step, &raw)?;
    let nx = spec.prepared.retained.extrusion.source_grid_shape[0]
        * spec.request.spatial_refinement as usize;
    let height = (spec.prepared.retained.extrusion.source_grid_shape[1] - 2)
        * spec.request.spatial_refinement as usize;
    let mut values = Vec::new();
    for [i, j] in &request.points {
        if *i >= nx || *j == 0 || *j > height {
            return Err(invalid("exact original active cooling subcontrol required"));
        }
        let node = &nodes[(j - 1) * nx + i];
        values.push(serde_json::json!({"i":node.i,"j":node.j,"original_parent":node.parent,"position_m":[node.xy[0],node.xy[1],spec.request.initialization.retained.destination_origin_m[2]],"water_fraction":node.water_fraction,"specific_enthalpy_j_kg":node.enthalpy,"temperature_k":node.temperature,"liquid_fraction":node.liquid_fraction}));
    }
    Ok(
        serde_json::json!({"schema_version":1,"job_id":request.job_id,"science_id":plan.science_id()?,"execution_id":job.plan_digest,"execution_binding_digest":crate::contracts::digest(&store.execution_binding(&request.job_id)?)?,"region":request.region,"physical_time_s":request.physical_time_s,"native_step":step,"source":spec,"original_receipt":receipt_record,"original_field":record,"complete_history":receipt["independent_verification"]["observations"],"values":values,"units":{"position":"m","water_fraction":"1","specific_enthalpy":"J/kg","temperature":"K","liquid_fraction":"1"},"association":crate::cooling_fields::ASSOCIATION,"convergence":"separate complete-history spatial/temporal and uniform analytic gates required","physical_validation":"unqualified","scope":"original stationary synthetic retained-water conduction; no inferred ingress, expansion, pressure or freeze-thaw lifetime"}),
    )
}
