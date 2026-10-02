use assert_cmd::Command;

#[test]
fn sandbox_cli_exposes_the_lifecycle() {
    let output = Command::cargo_bin("harbor-rs")
        .unwrap()
        .args(["sandbox", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    for command in ["run", "watch", "status", "stop"] {
        assert!(help.contains(command));
    }
}

#[test]
fn sandbox_requires_explicit_profile_checkout_and_state() {
    Command::cargo_bin("harbor-rs")
        .unwrap()
        .args(["sandbox", "run"])
        .assert()
        .failure();
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing");
    Command::cargo_bin("harbor-rs")
        .unwrap()
        .arg("sandbox")
        .arg("status")
        .arg("--session")
        .arg(&missing)
        .assert()
        .failure();
    assert!(!missing.exists());
}
