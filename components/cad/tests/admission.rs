use harbor_cad::{contracts::*, storage::*, worker::load_profile};

fn profile() -> HostExecutionProfile {
    load_profile(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/profiles/ci.json"
    )))
    .unwrap()
}

#[test]
fn admitted_jobs_hold_capacity_across_restarts_and_retained_bytes_are_counted() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("state");
    let host = profile();
    let plan = ExecutionPlan::reference(CaseSpec::reference()).unwrap();
    let store = Store::open(&root).unwrap();
    let first = store.submit_with_profile(&plan, "first", &host).unwrap();
    let second = store.submit_with_profile(&plan, "second", &host).unwrap();
    assert!(store.try_start(&first.id, &host, 0).unwrap());
    assert!(!store.try_start(&second.id, &host, 0).unwrap());
    assert_eq!(store.job(&second.id).unwrap().state, "queued");
    drop(store);
    let store = Store::open(&root).unwrap();
    assert!(!store.try_start(&second.id, &host, 0).unwrap());
    store.finish(&first.id, 0, None).unwrap();
    assert!(
        !store
            .try_start(&second.id, &host, host.max_disk_bytes)
            .unwrap()
    );
    assert_eq!(store.job(&second.id).unwrap().state, "queued");
    assert!(store.try_start(&second.id, &host, 0).unwrap());
}

#[test]
fn effective_profiles_are_immutable_and_native_copy_space_is_reserved() {
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::open(&temporary.path().join("state")).unwrap();
    let host = profile();
    let plan = ExecutionPlan::reference(CaseSpec::reference()).unwrap();
    let job = store.submit_with_profile(&plan, "frozen", &host).unwrap();
    let mut changed = host.clone();
    changed.threads += 1;
    assert!(
        store
            .submit_with_profile(&plan, "frozen", &changed)
            .is_err()
    );
    assert_eq!(
        digest(&store.job_profile(&job.id).unwrap()).unwrap(),
        digest(&host).unwrap()
    );
    let mut native = CaseSpec::reference();
    native.acceleration.value = 0.001;
    native.applicability.formulation = "periodic_forced_channel".into();
    let native = ExecutionPlan::openlb_reference(native, "research".into()).unwrap();
    assert_eq!(
        native.disk_reservation().unwrap(),
        native.observation.max_artifact_bytes * 2
    );
    assert_eq!(
        plan.disk_reservation().unwrap(),
        plan.observation.max_artifact_bytes
    );
}
