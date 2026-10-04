use harbor_cad::storage::retain_failed_native_tree;
use harbor_cad::{contracts::*, storage::Store};

#[test]
fn failed_native_records_keep_closed_partial_bytes_and_report_unsafe_omissions() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("raw");
    let destination = temp.path().join("job");
    std::fs::create_dir_all(source.join("fields")).unwrap();
    std::fs::create_dir(&destination).unwrap();
    std::fs::write(source.join("flow.log"), b"numerical tolerance failed").unwrap();
    std::fs::write(source.join("fields/data.vti"), b"retained scientific bytes").unwrap();
    std::fs::write(source.join("receipt.json.partial"), b"unfinished json").unwrap();
    std::os::unix::fs::symlink("/etc/passwd", source.join("escape")).unwrap();
    let snapshot = retain_failed_native_tree(&source, &destination, 1024).unwrap();
    assert_eq!(snapshot.artifacts.len(), 3);
    assert_eq!(snapshot.skipped.len(), 1);
    assert_eq!(snapshot.skipped[0].path, "escape");
    assert!(!destination.join("failed-native/escape").exists());
    for item in &snapshot.artifacts {
        assert!(item.path.starts_with("failed-native/"));
        assert!(item.provenance.contains("failed"));
        assert!(item.units.is_none());
    }
    std::fs::write(source.join("fields/data.vti"), b"changed raw file").unwrap();
    assert_eq!(
        std::fs::read(destination.join("failed-native/fields/data.vti")).unwrap(),
        b"retained scientific bytes"
    );
    assert_eq!(
        std::fs::read(destination.join("failed-native/receipt.json.partial")).unwrap(),
        b"unfinished json"
    );
}

#[test]
fn over_budget_failure_is_preserved_raw_without_silent_export_truncation() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("raw");
    let destination = temp.path().join("job");
    std::fs::create_dir(&source).unwrap();
    std::fs::create_dir(&destination).unwrap();
    std::fs::write(source.join("large.vti"), vec![1u8; 1025]).unwrap();
    let snapshot = retain_failed_native_tree(&source, &destination, 1024).unwrap();
    assert!(snapshot.artifacts.is_empty());
    assert_eq!(snapshot.skipped[0].path, "large.vti");
    assert!(snapshot.skipped[0].reason.contains("budget"));
    assert_eq!(
        std::fs::metadata(source.join("large.vti")).unwrap().len(),
        1025
    );
}

#[test]
fn failed_job_exports_qualified_status_and_every_registered_diagnostic() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("state")).unwrap();
    let plan = ExecutionPlan::reference(CaseSpec::reference()).unwrap();
    let job = store.submit(&plan, "failed-export").unwrap();
    let raw = temp.path().join("raw");
    std::fs::create_dir(&raw).unwrap();
    std::fs::write(raw.join("tolerance.log"), b"approved gate failed").unwrap();
    let dir = store.job_dir(&job.id).unwrap();
    let snapshot = retain_failed_native_tree(&raw, &dir, 1024).unwrap();
    for item in &snapshot.artifacts {
        store.add_artifact(&job.id, item).unwrap();
    }
    store
        .transition(&job.id, "queued", "starting", None)
        .unwrap();
    let destination = temp.path().join("export");
    assert!(
        store.export(&job.id, &destination).is_err(),
        "active jobs must not be exported"
    );
    store
        .finish(&job.id, 1, Some("numerical tolerance failed"))
        .unwrap();
    assert_eq!(store.export(&job.id, &destination).unwrap(), 1);
    let execution: serde_json::Value =
        serde_json::from_slice(&std::fs::read(destination.join("execution.json")).unwrap())
            .unwrap();
    assert_eq!(execution["job"]["state"], "failed");
    assert_eq!(execution["job"]["error"], "numerical tolerance failed");
    assert_eq!(execution["physical_validation"], "unqualified");
    assert_eq!(
        execution["plan"]["case"]["resolution"],
        plan.case.resolution
    );
    assert_eq!(
        std::fs::read(destination.join("failed-native/tolerance.log")).unwrap(),
        b"approved gate failed"
    );
}

#[test]
fn historical_plan_keeps_its_original_inputs_when_current_applicability_rejects_it() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("state")).unwrap();
    let mut case = CaseSpec::reference();
    case.applicability.formulation = "periodic_forced_channel".into();
    case.acceleration.value = 0.001;
    let mut prior = ExecutionPlan::openlb_reference(case, "research".into()).unwrap();
    prior.case.acceleration.value = 0.1;
    let id = uuid::Uuid::new_v4().to_string();
    // Simulate an older worker that accepted this immutable plan and recorded
    // a failed execution. The new worker must forbid relaunch, while preserving
    // the historical records for diagnosis under the current qualification gate.
    store.connection.execute(
        "INSERT INTO jobs(id,idem,digest,plan,state,unit,created) VALUES(?1,'legacy',?2,?3,'failed',?4,0)",
        rusqlite::params![id, prior.id().unwrap(), serde_json::to_string(&prior).unwrap(), format!("harbor-cad-job-{id}.service")],
    ).unwrap();
    assert!(store.plan(&id).is_err());
    let destination = temp.path().join("archive");
    store.export(&id, &destination).unwrap();
    let execution: serde_json::Value =
        serde_json::from_slice(&std::fs::read(destination.join("execution.json")).unwrap())
            .unwrap();
    assert_eq!(execution["plan"]["case"]["acceleration"]["value"], 0.1);
    assert_eq!(execution["current_plan_check"]["accepted"], false);
    assert_eq!(
        execution["current_plan_check"]["diagnostic"]["code"],
        "invalid_input"
    );
    assert_eq!(execution["job"]["plan_digest"], prior.id().unwrap());
}
