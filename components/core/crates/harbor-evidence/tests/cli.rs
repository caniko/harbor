use std::{fs, process::Command};

use harbor_evidence::write_json_atomic;
use serde_json::json;

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_harbor-verify-artifact"))
}

#[test]
fn receipt_is_private_complete_and_never_clobbered_without_replace() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("receipt.json");
    write_json_atomic(&path, &json!({"status":"RUNNING"}), false).unwrap();
    let original = fs::read(&path).unwrap();
    assert!(write_json_atomic(&path, &json!({"status":"PASS"}), false).is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
    write_json_atomic(&path, &json!({"status":"FAILED"}), true).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&fs::read(&path).unwrap()).unwrap()["status"],
        "FAILED"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn compatibility_and_dev_prod_frontends_require_accepted_source_bound_proof() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("artifact");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("index.html"), "correct").unwrap();
    let manifest = directory.path().join("manifest.json");
    let roots = directory.path().join("roots.json");
    let report = directory.path().join("report.json");
    let required = directory.path().join("required.json");
    write_json_atomic(&roots, &json!({"artifact":root}), false).unwrap();
    write_json_atomic(
        &report,
        &json!({"tests":[{"id":"review","status":"passed"}]}),
        false,
    )
    .unwrap();
    write_json_atomic(&required, &json!(["review"]), false).unwrap();

    assert!(
        Command::new(env!("CARGO_BIN_EXE_harbor"))
            .args(["dev", "artifact", "snapshot", "--manifest"])
            .arg(&manifest)
            .arg("--roots")
            .arg(&roots)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        !binary()
            .args(["verify-local", "--manifest"])
            .arg(&manifest)
            .arg("--root")
            .arg(&root)
            .output()
            .unwrap()
            .status
            .success()
    );
    let original = fs::read(&manifest).unwrap();
    assert!(
        !binary()
            .args(["snapshot", "--manifest"])
            .arg(&manifest)
            .arg("--roots")
            .arg(&roots)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert_eq!(fs::read(&manifest).unwrap(), original);
    assert!(
        binary()
            .args(["accept", "--manifest"])
            .arg(&manifest)
            .arg("--report")
            .arg(&report)
            .arg("--required")
            .arg(&required)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new(env!("CARGO_BIN_EXE_harbor"))
            .args(["prod", "artifact", "verify-local", "--manifest"])
            .arg(&manifest)
            .arg("--root")
            .arg(&root)
            .status()
            .unwrap()
            .success()
    );
    fs::write(root.join("index.html"), "stale").unwrap();
    assert!(
        !binary()
            .args(["verify-local", "--manifest"])
            .arg(&manifest)
            .arg("--root")
            .arg(&root)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        !binary()
            .args(["verify-http", "--manifest"])
            .arg(&manifest)
            .args(["--url", "http://127.0.0.1:1", "--attempts", "0"])
            .output()
            .unwrap()
            .status
            .success()
    );
}
