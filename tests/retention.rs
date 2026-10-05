use harbor_cad::{contracts::*, execution::*, retention::*, storage::*, worker::load_profile};
use std::{collections::BTreeMap, fs, path::Path};

struct Registry {
    fail_at: Option<usize>,
    calls: usize,
}
impl GcRootRegistry for Registry {
    fn register(&mut self, root: &Path, target: &Path) -> harbor_cad::Result<()> {
        self.calls += 1;
        std::os::unix::fs::symlink(target, root)?;
        if self.fail_at == Some(self.calls) {
            return Err(harbor_cad::Error::Resource(
                "lost root acknowledgement".into(),
            ));
        }
        Ok(())
    }
}

fn binding() -> ExecutionBinding {
    ExecutionBinding {
        schema_version: 1,
        runner_protocol: 1,
        sandbox_policy: SANDBOX_POLICY.into(),
        plan_digest: "a".repeat(64),
        host_profile_digest: "b".repeat(64),
        runner: FileIdentity {
            path: "/nix/store/00000000000000000000000000000000-runner/bin/run".into(),
            sha256: "c".repeat(64),
            bytes: 1,
        },
        native_runtime: Some(FileIdentity {
            path: "/nix/store/11111111111111111111111111111111-runtime.json".into(),
            sha256: "d".repeat(64),
            bytes: 1,
        }),
        native_files: BTreeMap::new(),
    }
}

#[test]
fn root_registration_crash_before_database_commit_leaves_no_launchable_job_and_recovers_orphan() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("state")).unwrap();
    for fail_at in [1, 2] {
        let id = uuid::Uuid::new_v4().to_string();
        let mut registry = Registry {
            fail_at: Some(fail_at),
            calls: 0,
        };
        assert!(prepare_with(&store.root, &id, &binding(), true, &mut registry).is_err());
        assert!(verify_ready(&store.root, &id, &binding(), true).is_err());
        assert!(store.job(&id).is_err());
        assert!(store.root.join("retentions").join(&id).exists());
        store.cleanup_retention(|_| Ok(false)).unwrap();
        assert!(!store.root.join("retentions").join(&id).exists());
    }
    let id = uuid::Uuid::new_v4().to_string();
    prepare_with(
        &store.root,
        &id,
        &binding(),
        true,
        &mut Registry {
            fail_at: None,
            calls: 0,
        },
    )
    .unwrap();
    verify_ready(&store.root, &id, &binding(), true).unwrap();
    // Crash after registration but before SQLite insertion has the same orphan rule.
    store.cleanup_retention(|_| Ok(false)).unwrap();
    assert!(!store.root.join("retentions").join(&id).exists());
}

#[test]
fn committed_jobs_keep_retention_until_terminal_and_verified_tree_termination() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("state")).unwrap();
    let profile = load_profile(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/profiles/ci.json"
    )))
    .unwrap();
    let plan = ExecutionPlan::reference(CaseSpec::reference()).unwrap();
    let binding = ExecutionBinding::capture(
        &plan,
        &profile,
        Path::new(env!("CARGO_BIN_EXE_harbor-cad")),
        BTreeMap::new(),
    )
    .unwrap();
    let job = store
        .submit_for_execution(&plan, "durable", &profile, &binding)
        .unwrap();
    let dir = store.root.join("retentions").join(&job.id);
    verify_ready(&store.root, &job.id, &binding, false).unwrap();
    assert_eq!(
        store
            .submit_for_execution(&plan, "durable", &profile, &binding)
            .unwrap()
            .id,
        job.id
    );
    assert_eq!(
        fs::read_dir(store.root.join("retentions")).unwrap().count(),
        1
    );
    store
        .cleanup_retention(|_| panic!("queued retention cannot be released"))
        .unwrap();
    store.try_start(&job.id, &profile, 0).unwrap();
    store.finish(&job.id, 0, None).unwrap();
    store.cleanup_retention(|_| Ok(false)).unwrap();
    assert!(
        dir.exists(),
        "terminal database status is not tree termination proof"
    );
    drop(store);
    let store = Store::open(&temp.path().join("state")).unwrap();
    store.cleanup_retention(|_| Ok(true)).unwrap();
    assert!(!dir.exists());
    assert_eq!(
        digest(&store.execution_binding(&job.id).unwrap()).unwrap(),
        digest(&binding).unwrap()
    );
    store.cleanup_retention(|_| Ok(true)).unwrap();
}

#[test]
fn queued_cancellation_is_recoverable_without_releasing_another_job() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("state")).unwrap();
    let profile = load_profile(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/profiles/ci.json"
    )))
    .unwrap();
    let plan = ExecutionPlan::reference(CaseSpec::reference()).unwrap();
    let binding = ExecutionBinding::capture(
        &plan,
        &profile,
        Path::new(env!("CARGO_BIN_EXE_harbor-cad")),
        BTreeMap::new(),
    )
    .unwrap();
    let first = store
        .submit_for_execution(&plan, "cancel", &profile, &binding)
        .unwrap();
    let other = store
        .submit_for_execution(&plan, "keep", &profile, &binding)
        .unwrap();
    store
        .transition(&first.id, "queued", "cancelled", None)
        .unwrap();
    store
        .cleanup_retention(|job| Ok(job.id == first.id))
        .unwrap();
    assert!(!store.root.join("retentions").join(&first.id).exists());
    assert!(store.root.join("retentions").join(&other.id).exists());
}
