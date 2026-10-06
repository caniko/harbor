//! Exact retained-time thermal samples, verified against authoritative native DAT.
use crate::{
    Error, Result,
    contracts::{StageOperation, digest, invalid},
    qualification::{self, EvidenceState},
    results::{self, SampleLocation, SampleReport, SampleRequest, SampleValue, StaticField},
    storage::Store,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ThermalField {
    Temperature,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThermalSampleRequest {
    pub schema_version: u32,
    pub job_id: String,
    pub field: ThermalField,
    pub physical_time_s: f64,
    pub locations: Vec<SampleLocation>,
}
impl ThermalSampleRequest {
    pub fn validate(&self) -> Result<()> {
        self.static_request().validate()?;
        if !self.physical_time_s.is_finite() || self.physical_time_s <= 0. {
            return Err(invalid("positive explicit retained physical time required"));
        }
        Ok(())
    }
    fn static_request(&self) -> SampleRequest {
        SampleRequest {
            schema_version: self.schema_version,
            job_id: self.job_id.clone(),
            field: StaticField::Temperature,
            locations: self.locations.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThermalCompareRequest {
    pub schema_version: u32,
    pub left: ThermalSampleRequest,
    pub right: ThermalSampleRequest,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ThermalSampleReport {
    #[serde(flatten)]
    pub sample: SampleReport,
    pub native_time_s: f64,
    pub time_serialization_tolerance_s: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThermalCompareReport {
    pub schema_version: u32,
    pub left: ThermalSampleReport,
    pub right: ThermalSampleReport,
    pub differences: Vec<SampleValue>,
    pub maximum_abs_difference: f64,
    pub numerical_acceptance: String,
    pub physical_validation: String,
}

fn time_tolerance(time: f64) -> f64 {
    1e-7 * time.abs().max(1.)
}

fn verified_snapshot(
    request: &ThermalSampleRequest,
    mesh: &serde_json::Value,
    fields: &serde_json::Value,
    text: &str,
    resolution: u32,
    times: &[f64],
) -> Result<(usize, Vec<SampleValue>, f64)> {
    request.validate()?;
    let selected = times
        .iter()
        .position(|t| *t == request.physical_time_s)
        .ok_or_else(|| {
            invalid("exact approved retained physical time required; no interpolation")
        })?;
    if times.is_empty()
        || times.len() > 1200
        || fields["schema_version"] != 1
        || fields["transient"] != true
        || fields["coordinate_unit"] != "m"
        || fields["fields"].as_object().is_none_or(|v| v.len() != 1)
    {
        return Err(invalid(
            "complete registered SI transient temperature history required",
        ));
    }
    let snapshots = fields["fields"]["temperature"]
        .as_array()
        .filter(|v| v.len() == times.len())
        .ok_or_else(|| invalid("complete approved thermal snapshot schedule required"))?;
    if text
        .lines()
        .any(|line| line.contains(" for set ") && !line.trim().starts_with("temperatures for set "))
    {
        return Err(invalid(
            "only authoritative thermal temperature sections supported",
        ));
    }
    let native = results::native_snapshots(&StaticField::Temperature, text)?;
    if native.len() != times.len() {
        return Err(invalid("complete native thermal schedule required"));
    }
    let source = request.static_request();
    let mut chosen = None;
    for (index, ((snapshot, expected), (actual, values))) in
        snapshots.iter().zip(times).zip(native).enumerate()
    {
        if snapshot["physical_time_s"].as_f64() != Some(*expected)
            || !expected.is_finite()
            || *expected <= 0.
            || (actual - expected).abs() > time_tolerance(*expected)
            || snapshot["values"] != values
        {
            return Err(invalid(
                "registered thermal times/values differ from approved schedule or authoritative native output",
            ));
        }
        let (count, samples) = results::extract_values(&source, mesh, &values, resolution)?;
        if index == selected {
            chosen = Some((count, samples, actual));
        }
    }
    chosen.ok_or_else(|| invalid("requested thermal snapshot absent"))
}

pub fn sample(store: &Store, request: &ThermalSampleRequest) -> Result<ThermalSampleReport> {
    request.validate()?;
    let plan = store.plan(&request.job_id)?;
    let spec = plan
        .thermal
        .as_ref()
        .ok_or_else(|| Error::Unqualified("registered native thermal job required".into()))?;
    spec.validate()?;
    let evidence = qualification::inspect(store, &request.job_id)?;
    if evidence.job_state != "succeeded"
        || !evidence.capabilities.iter().any(|c| {
            c.operation == StageOperation::ThermalReference
                && matches!(c.runtime_execution, EvidenceState::Recorded)
                && matches!(c.numerical_verification, EvidenceState::ReportedPass)
        })
    {
        return Err(Error::Unqualified(
            "succeeded source-bound native thermal numerical result required".into(),
        ));
    }
    let (field_artifact, fields) =
        results::registered(store, &request.job_id, "stages/thermal/thermal-fields.json")?;
    let (mesh_artifact, mesh) =
        results::registered(store, &request.job_id, "stages/thermal/mesh.json")?;
    let (native_artifact, data) = results::registered_bytes(
        store,
        &request.job_id,
        "stages/thermal/reference.dat",
        "dat",
    )?;
    let text =
        std::str::from_utf8(&data).map_err(|_| invalid("native thermal output must be text"))?;
    let (source_samples, samples, native_time_s) = verified_snapshot(
        request,
        &mesh,
        &fields,
        text,
        spec.resolution,
        &spec.output_times(),
    )?;
    Ok(ThermalSampleReport {
        sample: SampleReport {
            schema_version: 1,
            job_id: request.job_id.clone(),
            science_id: evidence.science_id,
            execution_id: evidence.execution_id,
            execution_binding_digest: digest(&evidence.execution_binding.ok_or_else(|| {
                Error::Unqualified("native thermal execution binding required".into())
            })?)?,
            field: StaticField::Temperature,
            unit: "K".into(),
            components: 1,
            coordinate_unit: "m".into(),
            physical_time_s: Some(request.physical_time_s),
            field_artifact,
            native_artifact,
            mesh_artifact,
            source_samples,
            samples,
            interpolation: "none; exact native entity identity and approved retained time".into(),
            physical_validation: "unqualified".into(),
        },
        native_time_s,
        time_serialization_tolerance_s: time_tolerance(request.physical_time_s),
    })
}

pub fn compare(store: &Store, request: &ThermalCompareRequest) -> Result<ThermalCompareReport> {
    request.left.validate()?;
    request.right.validate()?;
    if request.schema_version != 1 || request.left.locations != request.right.locations {
        return Err(invalid("same ordered native thermal locations required"));
    }
    let left = sample(store, &request.left)?;
    let right = sample(store, &request.right)?;
    let compared = results::compare_reports(left.sample.clone(), right.sample.clone())?;
    Ok(ThermalCompareReport {
        schema_version: 1,
        left,
        right,
        differences: compared.differences,
        maximum_abs_difference: compared.maximum_abs_difference,
        numerical_acceptance: compared.numerical_acceptance,
        physical_validation: compared.physical_validation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::results::{SampleLocation, StaticField};
    use serde_json::json;

    fn fixture() -> (
        ThermalSampleRequest,
        serde_json::Value,
        serde_json::Value,
        String,
    ) {
        let request = ThermalSampleRequest {
            schema_version: 1,
            job_id: uuid::Uuid::new_v4().to_string(),
            field: ThermalField::Temperature,
            physical_time_s: 2.,
            locations: vec![
                SampleLocation::Node { node_id: 8 },
                SampleLocation::Node { node_id: 1 },
            ],
        };
        let mesh = json!({"nodes":(1..=8).map(|id|(id.to_string(),json!([id,0,0]))).collect::<std::collections::BTreeMap<_,_>>()});
        let fields = json!({"schema_version":1,"transient":true,"coordinate_unit":"m","fields":{"temperature":(1..=2).map(|t|json!({"physical_time_s":t,"values":(1..=8).map(|id|json!({"id":[id],"value":[293.+id as f64-t as f64]})).collect::<Vec<_>>()})).collect::<Vec<_>>()}});
        let mut native = String::new();
        for t in 1..=2 {
            native.push_str(&format!(
                "S T E P 1\nINCREMENT {t}\n temperatures for set NALL and time {t}.000000D+00\n"
            ));
            for id in 1..=8 {
                native.push_str(&format!("{id} {}D+00\n", 293 + id - t));
            }
        }
        (request, mesh, fields, native)
    }

    #[test]
    fn thermal_samples_select_exact_retained_time_and_preserve_native_ids() {
        let (request, mesh, fields, native) = fixture();
        let (count, samples, native_time) =
            verified_snapshot(&request, &mesh, &fields, &native, 1, &[1., 2.]).unwrap();
        assert_eq!(count, 8);
        assert_eq!(native_time, 2.);
        assert_eq!(samples[0].value, vec![299.]);
        assert_eq!(samples[1].value, vec![292.]);
        assert_eq!(request.static_request().field, StaticField::Temperature);
        let mut changed = request.clone();
        changed.physical_time_s = 1.5;
        assert!(verified_snapshot(&changed, &mesh, &fields, &native, 1, &[1., 2.]).is_err());
    }

    #[test]
    fn thermal_checks_all_native_snapshots_not_only_requested_nodes_and_time() {
        let (request, mesh, fields, native) = fixture();
        let mut changed = fields.clone();
        changed["fields"]["temperature"][0]["values"][3]["value"][0] = json!(999.);
        assert!(verified_snapshot(&request, &mesh, &changed, &native, 1, &[1., 2.]).is_err());
        for changed in [
            native.replace("NALL", "FOREIGN"),
            format!("{native}{native}"),
            native.replace("1.000000D+00", "1.010000D+00"),
            native.replace("4 296D", "1 296D"),
            native.replace("8 299D", "8 nanD"),
        ] {
            assert!(verified_snapshot(&request, &mesh, &fields, &changed, 1, &[1., 2.]).is_err());
        }
        let mut changed = fields.clone();
        changed["fields"]["temperature"][0]["values"]
            .as_array_mut()
            .unwrap()
            .pop();
        assert!(verified_snapshot(&request, &mesh, &changed, &native, 1, &[1., 2.]).is_err());
    }

    #[test]
    fn thermal_time_serialization_is_explicit_and_never_implicit_interpolation() {
        let (mut request, mesh, mut fields, native) = fixture();
        request.physical_time_s = 2.00000004;
        fields["fields"]["temperature"][1]["physical_time_s"] = json!(request.physical_time_s);
        assert_eq!(
            verified_snapshot(
                &request,
                &mesh,
                &fields,
                &native,
                1,
                &[1., request.physical_time_s]
            )
            .unwrap()
            .2,
            2.
        );
        request.physical_time_s += 1e-12;
        assert!(
            verified_snapshot(&request, &mesh, &fields, &native, 1, &[1., 2.00000004]).is_err()
        );
        for time in [0., -1., f64::NAN, f64::INFINITY] {
            request.physical_time_s = time;
            assert!(request.validate().is_err());
        }
    }
}
