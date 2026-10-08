#![cfg(target_os = "linux")]

use harbor_sandbox::{Desktop, Profile, Session, Tools};
use std::collections::BTreeMap;
use std::fs;

fn profile() -> Profile {
    Profile {
        schema_version: 1,
        name: "fixture".into(),
        tools: Tools {
            bwrap: "/missing/bwrap".into(),
            bash: "/bin/bash".into(),
            dbus: None,
            sway: None,
        },
        environment: BTreeMap::new(),
        shell_hook: String::new(),
        inputs: vec!["src".into(), "Cargo.lock".into()],
        design_copies: vec![],
        command: vec!["true".into()],
        services: vec![],
        build: None,
        ready: None,
        desktop: Desktop::None,
        network: false,
        gpu: false,
        cargo_config: None,
        poll_ms: 100,
        debounce_ms: 100,
        build_timeout_seconds: 5,
        ready_timeout_seconds: 2,
    }
}

#[test]
fn traversal_and_reserved_environment_are_rejected() {
    let mut p = profile();
    p.inputs.push("../private".into());
    assert!(p.validate().is_err());
    p.inputs.pop();
    p.environment.insert("HOME".into(), "/real/home".into());
    assert!(p.validate().is_err());
    p.environment.clear();
    p.command.clear();
    assert!(p.validate().is_err());
}

#[test]
fn snapshots_keep_the_source_and_lockfiles_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source");
    fs::create_dir_all(source.join("src")).unwrap();
    fs::write(source.join("src/main.rs"), "old").unwrap();
    fs::write(source.join("Cargo.lock"), "locked").unwrap();
    fs::write(source.join("unlisted-secret"), "private").unwrap();
    let session = Session::open(&profile(), &source, &tmp.path().join("state"), "a").unwrap();
    let first = session.snapshot(1).unwrap();
    fs::write(first.workspace.join("src/main.rs"), "generated").unwrap();
    assert_eq!(
        fs::read_to_string(source.join("src/main.rs")).unwrap(),
        "old"
    );
    assert!(!first.workspace.join("unlisted-secret").exists());
    fs::remove_file(source.join("src/main.rs")).unwrap();
    let second = session.snapshot(2).unwrap();
    assert!(!second.workspace.join("src/main.rs").exists());
    assert_eq!(
        fs::read_to_string(second.workspace.join("Cargo.lock")).unwrap(),
        "locked"
    );
}

#[test]
fn links_cannot_pull_private_files_into_a_snapshot() {
    use std::os::unix::fs::symlink;
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source");
    fs::create_dir_all(source.join("src")).unwrap();
    fs::write(source.join("Cargo.lock"), "locked").unwrap();
    symlink("../../private", source.join("src/escape")).unwrap();
    let session = Session::open(&profile(), &source, &tmp.path().join("state"), "a").unwrap();
    assert!(session.snapshot(1).is_err());
}

#[test]
fn exclusive_sessions_and_variant_state_are_distinct() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source");
    fs::create_dir(&source).unwrap();
    let state = tmp.path().join("state");
    let first = Session::open(&profile(), &source, &state, "a").unwrap();
    assert!(Session::open(&profile(), &source, &state, "a").is_err());
    let second = Session::open(&profile(), &source, &state, "b").unwrap();
    assert_ne!(first.directory(), second.directory());
    drop(first);
    assert!(Session::open(&profile(), &source, &state, "a").is_ok());
    assert!(Session::open(&profile(), &source, &source.join("state"), "c").is_err());
}
