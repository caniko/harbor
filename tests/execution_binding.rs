use harbor_cad::{contracts::*, execution::*, storage::*, worker::load_profile};
use std::{collections::BTreeMap, fs, path::Path};

fn profile() -> HostExecutionProfile {
    load_profile(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/profiles/ci.json"
    )))
    .unwrap()
}

#[test]
fn binding_rejects_changed_runner_and_unknown_policy_without_rewriting_approvals() {
    let temporary = tempfile::tempdir().unwrap();
    let runner = temporary.path().join("runner");
    fs::write(&runner, b"runner A").unwrap();
    let plan = ExecutionPlan::reference(CaseSpec::reference()).unwrap();
    let profile = profile();
    let binding = ExecutionBinding::capture(&plan, &profile, &runner, BTreeMap::new()).unwrap();
    binding.verify(&plan, &profile).unwrap();
    let mut upgraded = binding.clone();
    upgraded.schema_version = 2;
    assert!(upgraded.verify(&plan, &profile).is_err());
    upgraded = binding.clone();
    upgraded.sandbox_policy = "guessed-policy".into();
    assert!(upgraded.verify(&plan, &profile).is_err());
    fs::write(&runner, b"runner B").unwrap();
    assert!(binding.verify(&plan, &profile).is_err());
}

#[test]
fn legacy_plan_profile_and_export_keep_exact_identities_without_inventing_execution_binding() {
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::open(&temporary.path().join("state")).unwrap();
    let plan = ExecutionPlan::reference(CaseSpec::reference()).unwrap();
    let profile = profile();
    let job = store
        .submit_with_profile(&plan, "legacy", &profile)
        .unwrap();
    let prior: (String, String, String, String) = store.connection.query_row(
        "SELECT jobs.digest,jobs.plan,job_profiles.digest,job_profiles.profile FROM jobs JOIN job_profiles ON jobs.id=job_profiles.job WHERE jobs.id=?1",
        [&job.id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).unwrap();
    assert!(matches!(
        store.execution_binding(&job.id),
        Err(harbor_cad::Error::Unqualified(_))
    ));
    store
        .transition(&job.id, "queued", "starting", None)
        .unwrap();
    store.finish(&job.id, 1, Some("legacy record")).unwrap();
    let destination = temporary.path().join("export");
    store.export(&job.id, &destination).unwrap();
    let execution: serde_json::Value =
        serde_json::from_slice(&fs::read(destination.join("execution.json")).unwrap()).unwrap();
    assert!(execution["execution_binding"].is_null());
    assert_eq!(execution["job"]["plan_digest"], plan.id().unwrap());
    assert_eq!(digest(&store.plan(&job.id).unwrap()).unwrap(), prior.0);
    assert_eq!(
        digest(&store.job_profile(&job.id).unwrap()).unwrap(),
        prior.2
    );
    let after: (String, String, String, String) = store.connection.query_row(
        "SELECT jobs.digest,jobs.plan,job_profiles.digest,job_profiles.profile FROM jobs JOIN job_profiles ON jobs.id=job_profiles.job WHERE jobs.id=?1",
        [&job.id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).unwrap();
    assert_eq!(prior, after);
}
