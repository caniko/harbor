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
fn hip_binding_cannot_reuse_the_historical_generic_native_policy() {
    let temporary = tempfile::tempdir().unwrap();
    let runner = temporary.path().join("runner");
    fs::write(&runner, b"binding-only fixture; never executed").unwrap();
    let mut case = CaseSpec::reference();
    case.applicability.formulation = "periodic_forced_channel".into();
    case.acceleration.value = 0.001;
    let selection = |role, backend: &str, uuid| GpuSelection {
        role,
        backend: backend.into(),
        pci: "0000:03:00.0".into(),
        backend_uuid: uuid,
    };
    let plan = ExecutionPlan::b1(
        case,
        B1Selections {
            compute: selection(Role::Compute, "hip", Some("GPU-exact-fixture".into())),
            render: selection(Role::Render, "egl", None),
            media: selection(Role::Media, "vaapi", None),
        },
        "research".into(),
    )
    .unwrap();
    let approved = plan.id().unwrap();
    let mut profile = profile();
    profile.policy = "research".into();
    let mut binding = ExecutionBinding::capture(&plan, &profile, &runner, BTreeMap::new()).unwrap();
    assert_eq!(binding.sandbox_policy, HIP_SANDBOX_POLICY);
    binding.verify(&plan, &profile).unwrap();
    binding.sandbox_policy = SANDBOX_POLICY.into();
    assert!(binding.verify(&plan, &profile).is_err());
    assert_eq!(plan.id().unwrap(), approved);
}

#[test]
fn freezing_binding_cannot_reuse_wetting_or_generic_native_sandbox_policy() {
    let temporary = tempfile::tempdir().unwrap();
    let runner = temporary.path().join("runner");
    fs::write(&runner, b"binding-only fixture; never executed").unwrap();
    let plan = ExecutionPlan::freezing_reference(
        serde_json::from_str(include_str!("../examples/freezing-reference.json")).unwrap(),
        "research".into(),
    )
    .unwrap();
    let approved = plan.id().unwrap();
    let mut profile = profile();
    profile.policy = "research".into();
    let binding = ExecutionBinding::capture(&plan, &profile, &runner, BTreeMap::new()).unwrap();
    assert_eq!(binding.sandbox_policy, harbor_cad::freezing::SANDBOX_POLICY);
    binding.verify(&plan, &profile).unwrap();
    for foreign in [
        SANDBOX_POLICY,
        WETTING_SANDBOX_POLICY,
        THERMAL_SANDBOX_POLICY,
    ] {
        let mut changed = binding.clone();
        changed.sandbox_policy = foreign.into();
        assert!(changed.verify(&plan, &profile).is_err());
    }
    assert_eq!(plan.id().unwrap(), approved);
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
