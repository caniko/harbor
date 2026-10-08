//! Source-bound compute filtering of registered, retained science.
use crate::{
    Result,
    contracts::*,
    fields, presentation,
    storage::{Store, native_manifest},
};

pub fn plan(store: &Store, request: FilterRequest, policy: String) -> Result<ExecutionPlan> {
    let (_, snapshot, source) = presentation::source(store, &request.source_job)?;
    if snapshot
        .times
        .iter()
        .find(|t| t.requested_s == request.filter.time_s)
        .is_none_or(|t| t.shards.len() != 1)
    {
        return Err(invalid(
            "numerical filter requires one registered image shard at an exact retained time",
        ));
    }
    ExecutionPlan::numerical_filter(&store.plan(&request.source_job)?, source, request, policy)
}

pub(crate) fn descriptor(
    store: &Store,
    id: &str,
    plan: &ExecutionPlan,
) -> Result<serde_json::Value> {
    let filter = plan
        .filter
        .as_ref()
        .ok_or_else(|| invalid("filter parameters required"))?;
    let (_, snapshot, hash) = fields::registered(store, id)?;
    presentation::verify(plan, &snapshot, &hash)?;
    let time = snapshot
        .times
        .iter()
        .find(|t| t.requested_s == filter.time_s)
        .ok_or_else(|| invalid("source filter time missing"))?;
    if time.shards.len() != 1 {
        return Err(invalid("single image shard filter required"));
    }
    let file = snapshot
        .files
        .iter()
        .find(|f| f.path == time.shards[0])
        .ok_or_else(|| invalid("registered filter source missing"))?;
    if file.bytes == 0 || file.bytes > 64 * 1024 * 1024 || !file.path.ends_with(".vti") {
        return Err(invalid(
            "filter source exceeds supported image/input budget",
        ));
    }
    let (field, unit) = match filter.field {
        GradientField::Velocity => ("physVelocity", "m/s"),
        GradientField::Pressure => ("physPressure", "Pa"),
    };
    if snapshot.field_units[field] != unit {
        return Err(invalid(
            "gradient requires exact retained source field units",
        ));
    }
    let selection = plan
        .stages
        .iter()
        .find(|s| matches!(s.operation, StageOperation::NumericalFilter))
        .and_then(|s| s.selection.as_ref())
        .ok_or_else(|| invalid("filter compute selection required"))?;
    Ok(
        serde_json::json!({"schema_version":1,"selection":selection,"field":field,
        "source_file":file.path,"source_sha256":file.sha256,"source_snapshot_sha256":hash,"science_id":snapshot.science_id,"execution_id":snapshot.execution_id,
        "filter_execution_id":plan.id()?,"max_input_bytes":file.bytes,"max_points":1_000_000,"max_output_bytes":256*1024*1024}),
    )
}

pub(crate) fn verify_receipt(
    store: &Store,
    id: &str,
    plan: &ExecutionPlan,
    root: &std::path::Path,
    evidence: &serde_json::Value,
) -> Result<()> {
    let request = descriptor(store, id, plan)?;
    for key in [
        "source_sha256",
        "source_snapshot_sha256",
        "science_id",
        "execution_id",
        "filter_execution_id",
    ] {
        if evidence[key] != request[key] {
            return Err(invalid(
                "numerical-filter receipt differs from approved science/source identity",
            ));
        }
    }
    if evidence["source_field"] != request["field"]
        || evidence["association"] != "point"
        || evidence["precision"] != "float64"
        || evidence["coordinate_and_source_array_roundtrip"] != "exact_bytes"
        || evidence["hip_dispatches"].as_u64().is_none_or(|n| n == 0)
        || evidence["physical_validation"] != "unqualified"
    {
        return Err(invalid(
            "filter lacks required HIP, Float64 and round-trip evidence",
        ));
    }
    let (source_unit, output_unit) = if request["field"] == "physVelocity" {
        ("m/s", "1/s")
    } else {
        ("Pa", "Pa/m")
    };
    if evidence["source_unit"] != source_unit
        || evidence["output_unit"] != output_unit
        || evidence["points"]
            .as_u64()
            .is_none_or(|n| n == 0 || n > 1_000_000)
    {
        return Err(invalid(
            "filter field dimensions or physical unit mapping changed",
        ));
    }
    let output = native_manifest(
        root,
        "gradient.vti",
        256 * 1024 * 1024,
        "source-bound numerical gradient",
    )?;
    if evidence["output_sha256"] != output.sha256 || evidence["output_bytes"] != output.bytes {
        return Err(invalid("filter output differs from verified receipt"));
    }
    let stage = plan
        .stages
        .iter()
        .find(|s| matches!(s.operation, StageOperation::NumericalFilter))
        .ok_or_else(|| invalid("filter stage required"))?;
    if evidence["peak_process_rss_bytes"]
        .as_u64()
        .is_none_or(|n| n > stage.ram_bytes)
        || evidence["kokkos_hip_space_peak_tracked_bytes"]
            .as_u64()
            .is_none_or(|n| n > stage.vram_bytes)
    {
        return Err(invalid(
            "measured filter resources exceed approved stage estimates",
        ));
    }
    let (_, snapshot, _) = fields::registered(store, id)?;
    let filter = plan
        .filter
        .as_ref()
        .ok_or_else(|| invalid("filter parameters required"))?;
    let time = snapshot
        .times
        .iter()
        .find(|t| t.requested_s == filter.time_s)
        .ok_or_else(|| invalid("retained filter time missing"))?;
    let mut units = snapshot.field_units.clone();
    units["gradient"] = output_unit.into();
    crate::storage::commit_artifact(
        root,
        "field-description.json",
        &serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version":1,"source_science_id":snapshot.science_id,"source_execution_id":snapshot.execution_id,
            "source_artifact_id":snapshot.artifact_id,"source_snapshot_sha256":request["source_snapshot_sha256"],
            "filter_execution_id":plan.id()?,"data_file":"gradient.vti","data_sha256":output.sha256,"data_bytes":output.bytes,
            "association":"point","precision":"float64","field_units":units,"requested_s":time.requested_s,"observed_s":time.observed_s,"step":time.step,
            "numerical_verification":"device identity and exact array/coordinate round trip; gradient accuracy requires separate reference qualification",
            "convergence":"not_assessed","physical_validation":"unqualified"
        }))?,
        "json",
        "trusted numerical-gradient field identity/units/time descriptor",
    )?;
    Ok(())
}
