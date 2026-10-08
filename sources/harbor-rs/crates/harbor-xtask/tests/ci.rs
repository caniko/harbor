use std::fs;

use harbor_xtask::{TestRunner, load_harbor_ci_config};
use tempfile::tempdir;

#[test]
fn loads_workspace_harbor_ci_metadata() {
    let temp = tempdir().expect("temporary directory");
    fs::write(
        temp.path().join("Cargo.toml"),
        r#"[workspace]
members = []

[workspace.metadata.harbor-ci]
locked = false
all-features = false
exclude = ["external-tests"]
nextest-args = ["--profile", "ci"]
test-runner = "nextest"
docs = true
audit = true
deny = true
package = true
"#,
    )
    .unwrap();

    let config = load_harbor_ci_config(temp.path()).expect("harbor-ci config");
    assert!(!config.locked);
    assert!(!config.all_features);
    assert_eq!(config.excludes, ["external-tests"]);
    assert_eq!(config.nextest_args, ["--profile", "ci"]);
    assert_eq!(config.test_runner, TestRunner::Nextest);
    assert!(config.docs && config.audit && config.deny && config.package);
}

#[test]
fn rejects_incompatible_workspace_and_nextest_options() {
    let temp = tempdir().expect("temporary directory");
    fs::write(
        temp.path().join("Cargo.toml"),
        r#"[workspace]
members = []

[workspace.metadata.harbor-ci]
packages = ["app"]
exclude = ["external-tests"]
"#,
    )
    .unwrap();
    let error = load_harbor_ci_config(temp.path()).expect_err("overlapping selectors must fail");
    assert!(error.to_string().contains("packages and harbor-ci.exclude"));

    fs::write(
        temp.path().join("Cargo.toml"),
        r#"[workspace]
members = []

[workspace.metadata.harbor-ci]
nextest-args = ["--profile", "ci"]
"#,
    )
    .unwrap();
    let error = load_harbor_ci_config(temp.path()).expect_err("nextest args need nextest");
    assert!(error.to_string().contains("nextest-args requires"));
}
