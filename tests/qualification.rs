use harbor_cad::{contracts::*, qualification, storage::*, worker};
use std::{
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Worker(Child);
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn completed(root: &std::path::Path) -> (Worker, Store, Job) {
    let child = Command::new(env!("CARGO_BIN_EXE_harbor-cad"))
        .args(["worker", "--state"])
        .arg(root)
        .args([
            "--profile",
            concat!(env!("CARGO_MANIFEST_DIR"), "/profiles/ci.json"),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let guard = Worker(child);
    let socket = root.join("worker.sock");
    let start = Instant::now();
    while !socket.exists() {
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(10));
    }
    let plan = ExecutionPlan::reference(CaseSpec::reference()).unwrap();
    let reply = worker::request(
        &socket,
        Operation::Submit {
            approved_digest: plan.id().unwrap(),
            plan: Box::new(plan),
            idempotency_key: "evidence".into(),
        },
    )
    .unwrap();
    assert!(reply.ok, "{reply:?}");
    let id = reply.data.unwrap()["id"].as_str().unwrap().to_owned();
    let store = Store::open(root).unwrap();
    loop {
        let job = store.job(&id).unwrap();
        if job.state == "succeeded" {
            return (guard, store, job);
        }
        assert_ne!(job.state, "failed", "{job:?}");
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn historical_evidence_is_bound_to_one_job_runner_and_numerical_scope() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("state");
    let (_worker, store, job) = completed(&root);
    let before = std::fs::read(root.join("jobs.sqlite3")).unwrap();
    let report = serde_json::to_value(qualification::inspect(&store, &job.id).unwrap()).unwrap();
    assert_eq!(report["job_id"], job.id);
    assert_eq!(report["execution_id"], job.plan_digest);
    assert_eq!(report["scope"], "historical_job_evidence");
    assert_eq!(report["current_runtime_qualification"], "not_assessed");
    assert_eq!(report["physical_validation"], "unqualified");
    assert_eq!(report["capabilities"][0]["runtime_execution"], "recorded");
    assert_eq!(
        report["capabilities"][0]["numerical_verification"],
        "reported_pass"
    );
    assert_eq!(
        report["capabilities"][0]["formulation"],
        "steady_incompressible_channel"
    );
    assert_eq!(report["capabilities"][0]["dimensions"], 3);
    assert_eq!(report["capabilities"][0]["precision"], "float64");
    assert_eq!(
        report["execution_binding"]["runner"]["path"],
        env!("CARGO_BIN_EXE_harbor-cad")
    );
    assert_eq!(std::fs::read(root.join("jobs.sqlite3")).unwrap(), before);
    let cli = Command::new(env!("CARGO_BIN_EXE_harbor-cad"))
        .args(["--socket"])
        .arg(root.join("worker.sock"))
        .args(["qualify", "--job", &job.id])
        .output()
        .unwrap();
    assert!(
        cli.status.success(),
        "{}",
        String::from_utf8_lossy(&cli.stdout)
    );
    let response: serde_json::Value = serde_json::from_slice(&cli.stdout).unwrap();
    assert_eq!(response["data"], report);
}

#[test]
fn mutated_registered_evidence_fails_and_foreign_receipts_cannot_qualify_a_stage() {
    let temp = tempfile::tempdir().unwrap();
    let (_worker, store, job) = completed(&temp.path().join("state"));
    let root = store.job_dir(&job.id).unwrap();
    let fake = commit_artifact(&root, "openlb-receipt.json", br#"{"adapter":"OpenLB","backend":"hip","executed":true,"numerical_verification":{"passed":true}}"#, "json", "synthetic hostile unrelated receipt").unwrap();
    store.add_artifact(&job.id, &fake).unwrap();
    let report = serde_json::to_value(qualification::inspect(&store, &job.id).unwrap()).unwrap();
    assert!(
        !report["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["backend"] == "hip")
    );
    std::fs::write(
        root.join("validation.json"),
        br#"{"process":"succeeded","numerical_error":0,"physical_validation":"qualified"}"#,
    )
    .unwrap();
    assert!(qualification::inspect(&store, &job.id).is_err());
}

#[test]
fn exitless_legacy_job_cannot_turn_a_reported_pass_into_runtime_execution() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("state")).unwrap();
    let plan = ExecutionPlan::reference(CaseSpec::reference()).unwrap();
    let job = store.submit(&plan, "legacy").unwrap();
    let manifest = commit_artifact(
        &store.job_dir(&job.id).unwrap(),
        "validation.json",
        br#"{"process":"succeeded","numerical_error":0,"physical_validation":"unqualified"}"#,
        "json",
        "synthetic unexecuted fixture",
    )
    .unwrap();
    store.add_artifact(&job.id, &manifest).unwrap();
    let report = serde_json::to_value(qualification::inspect(&store, &job.id).unwrap()).unwrap();
    assert_eq!(
        report["capabilities"][0]["runtime_execution"],
        "not_observed"
    );
    assert_eq!(
        report["capabilities"][0]["numerical_verification"],
        "not_assessed"
    );
    assert!(report["execution_binding"].is_null());
}
