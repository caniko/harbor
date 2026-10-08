//! Bounded durable collections over ordinary immutable worker jobs.
use crate::{
    Error, Result,
    contracts::{
        ExecutionPlan, GpuRequirement, HostExecutionProfile, MAX_MESSAGE, StageOperation, digest,
        invalid, token,
    },
    storage::{Job, Store, now},
};
use rusqlite::{OptionalExtension, params};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StudyCase {
    pub name: String,
    pub plan: ExecutionPlan,
    pub approved_digest: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StudyRequest {
    pub schema_version: u32,
    pub name: String,
    pub provenance: String,
    pub cases: Vec<StudyCase>,
    pub max_total_artifact_bytes: u64,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct PreparedCase {
    pub name: String,
    pub plan_digest: String,
    pub science_id: String,
    pub max_artifact_bytes: u64,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct PreparedStudy {
    pub schema_version: u32,
    pub request_digest: String,
    pub cases: Vec<PreparedCase>,
    pub total_artifact_bytes: u64,
    pub executed: bool,
    pub physical_validation: String,
}

pub fn prepare(spec: &StudyRequest, policy: &str) -> Result<PreparedStudy> {
    if spec.schema_version != 1
        || !token(&spec.name)
        || !(2..=16).contains(&spec.cases.len())
        || spec.provenance.trim().is_empty()
        || spec.provenance.len() > 4096
        || serde_json::to_vec(spec)?.len() as u64 > MAX_MESSAGE - 2048
    {
        return Err(invalid(
            "bounded explicit study name, provenance and 2..16 cases required",
        ));
    }
    let mut names = BTreeSet::new();
    let mut plans = BTreeSet::new();
    let mut total = 0u64;
    let mut cases = vec![];
    for case in &spec.cases {
        let plan = &case.plan;
        plan.validate()?;
        let identity = plan.id()?;
        if !token(&case.name)
            || !names.insert(&case.name)
            || !plans.insert(identity.clone())
            || case.approved_digest != identity
            || plan.policy != policy
            || !matches!(plan.schema_version, 1 | 5 | 6 | 9 | 10 | 11 | 12 | 13 | 14)
            || plan.stages.iter().any(|stage| {
                stage.gpu != GpuRequirement::CpuOnly
                    || stage.selection.is_some()
                    || matches!(
                        stage.operation,
                        StageOperation::CadInspect
                            | StageOperation::CadMesh
                            | StageOperation::FemImported
                    )
            })
        {
            return Err(invalid(
                "unique named independently approved CPU study cases under the same policy required; retained-source recipes require independent staging",
            ));
        }
        total = total
            .checked_add(plan.observation.max_artifact_bytes)
            .ok_or_else(|| invalid("study output allowance overflow"))?;
        cases.push(PreparedCase {
            name: case.name.clone(),
            plan_digest: identity,
            science_id: plan.science_id()?,
            max_artifact_bytes: plan.observation.max_artifact_bytes,
        });
    }
    if spec.max_total_artifact_bytes < total
        || spec.max_total_artifact_bytes > 64 * 1024 * 1024 * 1024
    {
        return Err(invalid(
            "explicit study allowance must cover every case's unchanged output budget within 64 GiB",
        ));
    }
    Ok(PreparedStudy {
        schema_version: 1,
        request_digest: digest(spec)?,
        cases,
        total_artifact_bytes: total,
        executed: false,
        physical_validation: "unqualified".into(),
    })
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StudyIntent {
    pub schema_version: u32,
    pub id: String,
    pub request: StudyRequest,
    pub request_digest: String,
    pub profile: HostExecutionProfile,
    pub profile_digest: String,
    pub execution_digest: String,
    pub child_keys: Vec<String>,
    pub created_at: i64,
}

fn child_key(id: &str, case: &StudyCase) -> Result<String> {
    Ok(format!(
        "study_{}",
        digest(&(id, &case.name, &case.approved_digest))?
    ))
}

impl StudyIntent {
    fn validate(&self) -> Result<()> {
        let id = uuid::Uuid::parse_str(&self.id).map_err(|_| invalid("study UUID required"))?;
        prepare(&self.request, &self.profile.policy)?;
        if self.schema_version != 1
            || id.to_string() != self.id
            || self.request_digest != digest(&self.request)?
            || self.profile_digest != digest(&self.profile)?
            || self.created_at < 0
            || self.execution_digest.len() != 64
            || !self
                .execution_digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || self.child_keys
                != self
                    .request
                    .cases
                    .iter()
                    .map(|case| child_key(&self.id, case))
                    .collect::<Result<Vec<_>>>()?
        {
            return Err(invalid(
                "persisted study intent differs from original approval, profile or case identities",
            ));
        }
        Ok(())
    }
}

pub fn retain_intent(
    store: &Store,
    spec: &StudyRequest,
    key: &str,
    profile: &HostExecutionProfile,
    execution_digest: &str,
) -> Result<StudyIntent> {
    prepare(spec, &profile.policy)?;
    if !token(key) {
        return Err(invalid("bounded study idempotency key required"));
    }
    let tx = rusqlite::Transaction::new_unchecked(
        &store.connection,
        rusqlite::TransactionBehavior::Immediate,
    )?;
    let prior: Option<String> = tx
        .query_row("SELECT intent FROM studies WHERE idem=?1", [key], |row| {
            row.get(0)
        })
        .optional()?;
    if let Some(prior) = prior {
        let intent: StudyIntent = serde_json::from_str(&prior)?;
        intent.validate()?;
        if intent.request_digest != digest(spec)?
            || intent.profile_digest != digest(profile)?
            || intent.execution_digest != execution_digest
        {
            return Err(Error::IdempotencyConflict);
        }
        tx.commit()?;
        return Ok(intent);
    }
    let id = uuid::Uuid::new_v4().to_string();
    let intent = StudyIntent {
        schema_version: 1,
        child_keys: spec
            .cases
            .iter()
            .map(|case| child_key(&id, case))
            .collect::<Result<Vec<_>>>()?,
        id,
        request: spec.clone(),
        request_digest: digest(spec)?,
        profile: profile.clone(),
        profile_digest: digest(profile)?,
        execution_digest: execution_digest.into(),
        created_at: now(),
    };
    intent.validate()?;
    tx.execute(
        "INSERT INTO studies(id,idem,intent) VALUES(?1,?2,?3)",
        params![intent.id, key, serde_json::to_string(&intent)?],
    )?;
    tx.commit()?;
    Ok(intent)
}

#[derive(Debug, Serialize)]
pub struct StudyCaseStatus {
    pub name: String,
    pub plan_digest: String,
    pub science_id: String,
    pub job: Option<Job>,
}

#[derive(Debug, Serialize)]
pub struct StudyStatus {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub provenance: String,
    pub request_digest: String,
    pub profile_digest: String,
    pub execution_digest: String,
    pub cases: Vec<StudyCaseStatus>,
    pub submission: String,
    pub execution: String,
    pub physical_validation: String,
}

pub fn status(store: &Store, id: &str) -> Result<StudyStatus> {
    let raw: String = store
        .connection
        .query_row("SELECT intent FROM studies WHERE id=?1", [id], |row| {
            row.get(0)
        })
        .optional()?
        .ok_or_else(|| invalid("unknown study identity"))?;
    let intent: StudyIntent = serde_json::from_str(&raw)?;
    intent.validate()?;
    if intent.id != id {
        return Err(invalid("study database key differs from original identity"));
    }
    let mut cases = vec![];
    for (case, key) in intent.request.cases.iter().zip(&intent.child_keys) {
        cases.push(StudyCaseStatus {
            name: case.name.clone(),
            plan_digest: case.approved_digest.clone(),
            science_id: case.plan.science_id()?,
            job: store.existing_submission(&case.plan, key, &intent.profile)?,
        });
    }
    let complete = cases.iter().all(|case| case.job.is_some());
    let terminal = complete
        && cases.iter().all(|case| {
            case.job.as_ref().is_some_and(|job| {
                matches!(
                    job.state.as_str(),
                    "succeeded" | "failed" | "cancelled" | "interrupted"
                )
            })
        });
    let succeeded = terminal
        && cases.iter().all(|case| {
            case.job
                .as_ref()
                .is_some_and(|job| job.state == "succeeded")
        });
    Ok(StudyStatus {
        schema_version: 1,
        id: intent.id,
        name: intent.request.name,
        provenance: intent.request.provenance,
        request_digest: intent.request_digest,
        profile_digest: intent.profile_digest,
        execution_digest: intent.execution_digest,
        cases,
        submission: if complete { "complete" } else { "incomplete" }.into(),
        execution: if !complete {
            "incomplete_submission"
        } else if !terminal {
            "pending"
        } else if succeeded {
            "completed_successfully"
        } else {
            "completed_with_failures"
        }
        .into(),
        physical_validation: "unqualified".into(),
    })
}
