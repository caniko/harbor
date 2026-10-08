use std::fs;

use harbor_xtask::rewrite_spec_version;
use semver::Version;
use tempfile::tempdir;

#[test]
fn rewrites_only_the_single_version_line() {
    let temp_dir = tempdir().expect("temp dir");
    let spec_path = temp_dir.path().join("project.spec");
    let original = include_str!("fixtures/sample.spec");
    fs::write(&spec_path, original).expect("write fixture");

    rewrite_spec_version(&spec_path, &Version::parse("1.2.3").expect("version")).expect("rewrite");

    let rewritten = fs::read_to_string(&spec_path).expect("read rewritten spec");
    assert_eq!(
        rewritten,
        "Name:           sample\nVersion:        1.2.3\nRelease:        1%{?dist}\nSummary:        Sample package\n"
    );
}

#[test]
fn rejects_multiple_version_lines() {
    let temp_dir = tempdir().expect("temp dir");
    let spec_path = temp_dir.path().join("project.spec");
    fs::write(&spec_path, "Version: 1.0.0\nVersion: 2.0.0\n").expect("write fixture");

    let error = rewrite_spec_version(&spec_path, &Version::parse("3.0.0").expect("version"))
        .expect_err("rewrite should fail");

    assert!(
        error
            .to_string()
            .contains("expected exactly one Version: line")
    );
}
