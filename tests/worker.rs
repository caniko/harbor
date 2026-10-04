use harbor_cad::{contracts::*, storage::*, worker::*};
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
fn start(root: &std::path::Path) -> Worker {
    let child = Command::new(env!("CARGO_BIN_EXE_harbor-cad"))
        .args(["worker", "--state"])
        .arg(root)
        .arg("--profile")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/profiles/ci.json"))
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let deadline = Instant::now();
    while !root.join("worker.sock").exists() {
        assert!(deadline.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(10));
    }
    Worker(child)
}
#[test]
fn real_worker_disconnect_idempotency_restart_and_export() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("state");
    let worker = start(&root);
    let socket = root.join("worker.sock");
    let mut plan = ExecutionPlan::reference(CaseSpec::reference()).unwrap();
    plan.stages.push(Stage {
        id: "bundle".into(),
        dependencies: vec!["reference".into()],
        operation: StageOperation::Bundle,
        gpu: GpuRequirement::CpuOnly,
        selection: None,
        ram_bytes: 1048576,
        vram_bytes: 0,
    });
    let submit = Operation::Submit {
        approved_digest: plan.id().unwrap(),
        plan: Box::new(plan.clone()),
        idempotency_key: "disconnect-fixture".into(),
    };
    let response = request(&socket, submit.clone()).unwrap();
    assert!(response.ok, "{response:?}");
    let id = response.data.unwrap()["id"].as_str().unwrap().to_owned();
    let deadline = Instant::now();
    loop {
        let response = request(&socket, Operation::Status { job_id: id.clone() }).unwrap();
        let data = response.data.unwrap();
        if data["state"] == "succeeded" {
            break;
        }
        assert!(data["state"] != "failed", "{data}");
        assert!(deadline.elapsed() < Duration::from_secs(10), "{data}");
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(worker);
    std::fs::remove_file(&socket).unwrap();
    let _worker = start(&root);
    assert_eq!(request(&socket, submit).unwrap().data.unwrap()["id"], id);
    let artifacts = request(&socket, Operation::Artifacts { job_id: id.clone() })
        .unwrap()
        .data
        .unwrap();
    assert_eq!(artifacts.as_array().unwrap().len(), 4);
    let destination = temp.path().join("portable");
    let export = Command::new(env!("CARGO_BIN_EXE_harbor-cad"))
        .args(["artifact", "export", "--state"])
        .arg(&root)
        .arg(&id)
        .arg(&destination)
        .output()
        .unwrap();
    assert!(
        export.status.success(),
        "{}",
        String::from_utf8_lossy(&export.stdout)
    );
    assert!(destination.join("channel.csv").is_file());
    assert!(destination.join("manifest.json").is_file());
    let store = Store::open(&root).unwrap();
    assert!(store.logs(&id, 0, 101).is_err());
    assert_eq!(store.active().unwrap().len(), 0);
}

#[test]
fn worker_rejects_approval_drift_unknown_operations_and_gpu_fallback() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("state");
    let _worker = start(&root);
    let socket = root.join("worker.sock");
    let plan = ExecutionPlan::reference(CaseSpec::reference()).unwrap();
    let response = request(
        &socket,
        Operation::Submit {
            plan: Box::new(plan),
            approved_digest: "bad".into(),
            idempotency_key: "a".into(),
        },
    )
    .unwrap();
    assert!(!response.ok);
    assert_eq!(response.error.unwrap()["code"], "invalid_input");
    use std::io::{BufRead, Write};
    let mut stream = std::os::unix::net::UnixStream::connect(&socket).unwrap();
    stream.write_all(b"{\"protocol_version\":1,\"request_id\":\"x\",\"request\":{\"operation\":\"shell\",\"command\":\"id\"}}\n").unwrap();
    let mut reply = String::new();
    std::io::BufReader::new(stream)
        .read_line(&mut reply)
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&reply).unwrap()["ok"],
        false
    );
    assert!(Store::open(&root).unwrap().active().unwrap().is_empty());
}
