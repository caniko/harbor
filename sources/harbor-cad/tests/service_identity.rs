use harbor_cad::{contracts::*, storage::Store, worker::load_profile};
use std::{fs, path::Path, process::Command};

#[test]
fn invocation_environment_alone_cannot_authorize_a_service_job() {
    let temporary = tempfile::tempdir().unwrap();
    let state = temporary.path().join("state");
    let mut profile = load_profile(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/profiles/ci.json"
    )))
    .unwrap();
    profile.service_mode = "systemd".into();
    let plan = ExecutionPlan::reference(CaseSpec::reference()).unwrap();
    let store = Store::open(&state).unwrap();
    let job = store
        .submit_with_profile(&plan, "forged-invocation", &profile)
        .unwrap();
    assert!(store.try_start(&job.id, &profile, 0).unwrap());
    let configuration = temporary.path().join("profile.json");
    fs::write(&configuration, serde_json::to_vec(&profile).unwrap()).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_harbor-cad"))
        .args(["run-job", "--state"])
        .arg(&state)
        .arg("--profile")
        .arg(configuration)
        .arg(&job.id)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("INVOCATION_ID", "a".repeat(32))
        .output()
        .unwrap();
    assert!(
        !result.status.success(),
        "forged invocation executed the scientific job"
    );
    assert_eq!(store.job(&job.id).unwrap().state, "starting");
    assert!(store.artifacts(&job.id).unwrap().is_empty());
}
