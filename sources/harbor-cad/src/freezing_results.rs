//! Bounded exact-time original freezing point queries; no solver or interpolation.
use crate::{
    Error, Result,
    contracts::{StageOperation, digest, invalid},
    freezing::FreezingReferenceSpec,
    qualification::{self, EvidenceRecord, EvidenceState},
    storage::Store,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FreezingField {
    SpecificEnthalpy,
    Temperature,
    LiquidFraction,
}
impl FreezingField {
    fn metadata(&self) -> (usize, &'static str) {
        match self {
            Self::SpecificEnthalpy => (0, "J/kg"),
            Self::Temperature => (1, "K"),
            Self::LiquidFraction => (2, "1"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreezingSampleRequest {
    pub schema_version: u32,
    pub job_id: String,
    pub field: FreezingField,
    pub physical_time_s: f64,
    /// Original zero-based native (i,j) grid identities, in caller order.
    pub points: Vec<[u32; 2]>,
}
impl FreezingSampleRequest {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1
            || uuid::Uuid::parse_str(&self.job_id).is_err()
            || !self.physical_time_s.is_finite()
            || self.physical_time_s < 0.
            || self.points.is_empty()
            || self.points.len() > 64
            || self.points.iter().collect::<BTreeSet<_>>().len() != self.points.len()
        {
            return Err(invalid(
                "versioned freezing field, exact nonnegative physical time and 1-64 distinct native grid points required",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreezingPointValue {
    pub native_point: [u32; 2],
    pub coordinates_m: [f64; 3],
    pub value: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreezingSampleReport {
    pub schema_version: u32,
    pub job_id: String,
    pub science_id: String,
    pub execution_id: String,
    pub execution_binding_digest: String,
    pub field: FreezingField,
    pub unit: String,
    pub physical_time_s: f64,
    pub native_step: u64,
    pub source_samples: usize,
    pub association: String,
    pub field_artifact: EvidenceRecord,
    pub receipt_artifact: EvidenceRecord,
    pub samples: Vec<FreezingPointValue>,
    pub interpolation: String,
    pub physical_validation: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreezingCompareRequest {
    pub schema_version: u32,
    pub left: FreezingSampleRequest,
    pub right: FreezingSampleRequest,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreezingCompareReport {
    pub schema_version: u32,
    pub left: FreezingSampleReport,
    pub right: FreezingSampleReport,
    pub differences: Vec<FreezingPointValue>,
    pub maximum_abs_difference: f64,
    pub numerical_acceptance: String,
    pub physical_validation: String,
}

fn extract(
    spec: &FreezingReferenceSpec,
    request: &FreezingSampleRequest,
    bytes: &[u8],
) -> Result<(u64, usize, Vec<FreezingPointValue>)> {
    request.validate()?;
    let scale = spec.scale()?;
    let step = spec
        .observation_steps
        .iter()
        .zip(spec.times_s()?)
        .find(|(_, time)| *time == request.physical_time_s)
        .map(|(step, _)| *step)
        .ok_or_else(|| {
            invalid("one exact approved retained freezing time required; no interpolation")
        })?;
    let values = crate::freezing_fields::native_values(spec, step, bytes)?;
    let mut samples = Vec::with_capacity(request.points.len());
    for [i, j] in &request.points {
        if *i >= spec.resolution || *j >= spec.resolution / 8 {
            return Err(invalid(
                "requested native freezing point is outside complete original grid",
            ));
        }
        let value = values[*j as usize * spec.resolution as usize + *i as usize]
            [request.field.metadata().0];
        samples.push(FreezingPointValue {
            native_point: [*i, *j],
            coordinates_m: [
                f64::from(*i) * scale.spacing_m,
                (f64::from(*j) + 0.5) * scale.spacing_m,
                0.,
            ],
            value,
        });
    }
    Ok((step, values.len(), samples))
}

pub fn sample(store: &Store, request: &FreezingSampleRequest) -> Result<FreezingSampleReport> {
    request.validate()?;
    let job = store.job(&request.job_id)?;
    let plan = store.recorded_plan(&request.job_id)?;
    let spec = plan.freezing.as_ref().ok_or_else(|| {
        Error::Unqualified("registered conduction solidification job required".into())
    })?;
    let evidence = qualification::inspect(store, &request.job_id)?;
    if job.state != "succeeded"
        || job.exit_code != Some(0)
        || !evidence.capabilities.iter().any(|v| {
            v.operation == StageOperation::FreezingReference
                && matches!(v.runtime_execution, EvidenceState::Recorded)
                && matches!(v.numerical_verification, EvidenceState::ReportedPass)
        })
    {
        return Err(Error::Unqualified(
            "complete succeeded execution-bound original-field freezing evidence required".into(),
        ));
    }
    let (receipt_artifact, _) = crate::results::registered(
        store,
        &request.job_id,
        "stages/freezing/freezing-receipt.json",
    )?;
    let selected = spec
        .observation_steps
        .iter()
        .zip(spec.times_s()?)
        .find(|(_, time)| *time == request.physical_time_s)
        .map(|(step, _)| *step)
        .ok_or_else(|| invalid("exact retained freezing time required"))?;
    let path = format!("stages/freezing/freezing-{selected}.csv");
    let (field_artifact, bytes) =
        crate::results::registered_bytes(store, &request.job_id, &path, "csv")?;
    let (step, count, samples) = extract(spec, request, &bytes)?;
    Ok(FreezingSampleReport {
        schema_version: 1,
        job_id: job.id,
        science_id: plan.science_id()?,
        execution_id: job.plan_digest,
        execution_binding_digest: digest(&store.execution_binding(&request.job_id)?)?,
        field: request.field.clone(),
        unit: request.field.metadata().1.into(),
        physical_time_s: request.physical_time_s,
        native_step: step,
        source_samples: count,
        association: "native_lattice_point".into(),
        field_artifact,
        receipt_artifact,
        samples,
        interpolation: "none; exact native Float64 grid values and retained physical time".into(),
        physical_validation: "unqualified".into(),
    })
}

pub fn compare(store: &Store, request: &FreezingCompareRequest) -> Result<FreezingCompareReport> {
    request.left.validate()?;
    request.right.validate()?;
    if request.schema_version != 1
        || request.left.field != request.right.field
        || request.left.physical_time_s != request.right.physical_time_s
        || request.left.points != request.right.points
    {
        return Err(invalid(
            "matching freezing field, exact time and ordered native points required",
        ));
    }
    let left_plan = store.recorded_plan(&request.left.job_id)?;
    let right_plan = store.recorded_plan(&request.right.job_id)?;
    let left_geometry = left_plan
        .freezing
        .as_ref()
        .ok_or_else(|| invalid("left freezing geometry required"))?;
    let right_geometry = right_plan
        .freezing
        .as_ref()
        .ok_or_else(|| invalid("right freezing geometry required"))?;
    if left_geometry.resolution != right_geometry.resolution
        || left_geometry.size_m != right_geometry.size_m
    {
        return Err(invalid(
            "identical original freezing grid, SI coordinates and control volumes required; no implicit remeshing",
        ));
    }
    let left = sample(store, &request.left)?;
    let right = sample(store, &request.right)?;
    let mut maximum: f64 = 0.;
    let mut differences = Vec::new();
    for (a, b) in left.samples.iter().zip(&right.samples) {
        let difference = b.value - a.value;
        if !difference.is_finite()
            || a.native_point != b.native_point
            || a.coordinates_m != b.coordinates_m
        {
            return Err(invalid(
                "finite same-point SI original freezing comparison required",
            ));
        }
        maximum = maximum.max(difference.abs());
        differences.push(FreezingPointValue {
            native_point: a.native_point,
            coordinates_m: a.coordinates_m,
            value: difference,
        });
    }
    Ok(FreezingCompareReport {
        schema_version: 1,
        left,
        right,
        differences,
        maximum_abs_difference: maximum,
        numerical_acceptance:
            "not assessed; signed right-minus-left comparison without an invented engineering gate"
                .into(),
        physical_validation: "unqualified".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_original_points_preserve_float64_order_units_and_retained_time_without_interpolation()
    {
        let (root, spec, _) = crate::freezing_fields::tests::fixture();
        let mut request = FreezingSampleRequest {
            schema_version: 1,
            job_id: "63049501-0229-4057-9ee0-b8e1ff65206d".into(),
            field: FreezingField::Temperature,
            physical_time_s: spec.times_s().unwrap()[1],
            points: vec![[31, 3], [0, 0], [3, 1]],
        };
        let bytes = std::fs::read(root.path().join("freezing-1024.csv")).unwrap();
        let (step, count, samples) = extract(&spec, &request, &bytes).unwrap();
        assert_eq!((step, count), (1024, 128));
        assert_eq!(
            samples.iter().map(|s| s.native_point).collect::<Vec<_>>(),
            request.points
        );
        assert_eq!(samples[0].value, 273.15);
        assert_eq!(samples[1].value, 263.15);
        assert_eq!(
            serde_json::from_slice::<Vec<FreezingPointValue>>(
                &serde_json::to_vec(&samples).unwrap()
            )
            .unwrap()[2]
                .value
                .to_bits(),
            samples[2].value.to_bits()
        );
        request.field = FreezingField::LiquidFraction;
        assert_eq!(
            extract(&spec, &request, &bytes)
                .unwrap()
                .2
                .iter()
                .map(|v| v.value)
                .collect::<Vec<_>>(),
            vec![1., 0., 0.]
        );
        request.field = FreezingField::SpecificEnthalpy;
        assert_eq!(
            extract(&spec, &request, &bytes).unwrap().2[0].value,
            110000.
        );
        request.physical_time_s *= 0.5;
        assert!(extract(&spec, &request, &bytes).is_err());
        request.physical_time_s = spec.times_s().unwrap()[1];
        request.points = vec![[32, 0]];
        assert!(extract(&spec, &request, &bytes).is_err());
        request.points = vec![[1, 4]];
        assert!(extract(&spec, &request, &bytes).is_err());
        request.points = vec![[1, 0], [1, 0]];
        assert!(request.validate().is_err());
        request.points = vec![[1, 0]];
        assert!(extract(&spec, &request, &bytes[..bytes.len() - 150]).is_err());
    }
}
