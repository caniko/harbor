//! Real namespace acceptance. Required in hosted Linux CI; run locally only with
//! explicit `HARBOR_TEST_*` tool paths. No ambient host tool/PATH substitution.
#![cfg(target_os = "linux")]
mod common;
use common::{Supervisor, profile, source, wait_for};
use harbor_sandbox::{Desktop, RunOptions, Session, run, status, stop};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, atomic::AtomicBool};
use std::thread;
use std::time::{Duration, Instant};

#[test]
#[ignore = "requires explicitly provisioned Bubblewrap and Linux user namespaces"]
fn enforced_boundaries_private_hooks_and_writable_design_copies() {
    let tmp = tempfile::tempdir().unwrap();
    let source = source(tmp.path());
    let secret = tmp.path().join("host-secret");
    fs::write(&secret, "secret").unwrap();
    let mut profile = profile();
    profile.command = vec![
        "bash".into(),
        "-c".into(),
        format!(
            "test ! -e '{}' && test ! -e /dev/dri && test ! -e /dev/input && test \"$(readlink /proc/self/ns/net)\" != '{}' && test \"$DERIVED_DATA\" = /harbor/state/data/fixture && test -z \"${{AWS_SECRET_ACCESS_KEY:-}}\" && ! printf changed > /workspace/design && printf copy > \"$HARBOR_ARTIFACTS/designs/design\" && sleep 0.4",
            secret.display(),
            fs::read_link("/proc/self/ns/net").unwrap().display()
        ),
    ];
    let session = Session::open(&profile, &source, &tmp.path().join("state"), "a").unwrap();
    let directory = session.directory().to_owned();
    assert_eq!(
        run(
            session,
            RunOptions {
                watch: false,
                parent_wayland: None,
                cancel: Arc::new(AtomicBool::new(false))
            }
        )
        .unwrap(),
        0
    );
    assert_eq!(fs::read_to_string(source.join("design")).unwrap(), "good");
    assert_eq!(
        fs::read_to_string(directory.join("runtime/artifacts/1/designs/design")).unwrap(),
        "copy"
    );
    assert_eq!(fs::read_to_string(&secret).unwrap(), "secret");
}

#[test]
#[ignore = "requires explicitly provisioned Bubblewrap and Linux user namespaces"]
fn failed_build_keeps_preview_and_stop_is_variant_scoped() {
    let tmp = tempfile::tempdir().unwrap();
    let source = source(tmp.path());
    let mut profile = profile();
    profile.build = Some(vec![
        "bash".into(),
        "-c".into(),
        "test \"$(cat design)\" != bad && printf warm >> \"$CARGO_TARGET_DIR/reuse\"".into(),
    ]);
    profile.ready = Some(vec![
        "bash".into(),
        "-c".into(),
        "test -f \"$CARGO_TARGET_DIR/reuse\" && test \"$(cat design)\" != unready".into(),
    ]);
    profile.ready_timeout_seconds = 1;
    profile.command = vec!["bash".into(), "-c".into(), "trap 'printf graceful > \"$HARBOR_ARTIFACTS/stopped\"; exit 0' TERM; while :; do printf tick >> \"$HARBOR_ARTIFACTS/heartbeat\"; sleep 0.05; done".into()];
    let state = tmp.path().join("state");
    let a = Session::open(&profile, &source, &state, "a").unwrap();
    let b = Session::open(&profile, &source, &state, "b").unwrap();
    let dir_a = a.directory().to_owned();
    let dir_b = b.directory().to_owned();
    let worker_a = Supervisor::start(a, None);
    let worker_b = Supervisor::start(b, None);
    wait_for(&dir_a, |r| r.phase == "running");
    wait_for(&dir_b, |r| r.phase == "running");
    fs::write(source.join("design"), "bad").unwrap();
    wait_for(&dir_a, |r| r.phase == "build-failed");
    let receipt = status(&dir_a).unwrap();
    assert_eq!(receipt.preview_generation, Some(1));
    fs::write(source.join("design"), "unready").unwrap();
    wait_for(&dir_a, |r| r.phase == "preview-failed");
    assert_eq!(status(&dir_a).unwrap().preview_generation, Some(1));
    fs::write(source.join("design"), "fixed").unwrap();
    wait_for(&dir_a, |r| r.successful_switches == 2);
    let receipt = status(&dir_a).unwrap();
    assert_eq!(
        fs::read_to_string(
            dir_a
                .join("build")
                .join(receipt.environment_hash)
                .join("target/reuse")
        )
        .unwrap(),
        "warmwarmwarm"
    );
    stop(&dir_a).unwrap();
    let artifact = dir_a
        .join("runtime/artifacts")
        .join(receipt.preview_generation.unwrap().to_string());
    assert_eq!(
        fs::read_to_string(artifact.join("stopped")).unwrap(),
        "graceful"
    );
    let final_heartbeat = fs::read(artifact.join("heartbeat")).unwrap();
    thread::sleep(Duration::from_millis(150));
    assert_eq!(
        fs::read(artifact.join("heartbeat")).unwrap(),
        final_heartbeat
    );
    assert!(status(&dir_b).unwrap().active);
    stop(&dir_b).unwrap();
    worker_a.finish();
    worker_b.finish();
}

#[test]
#[ignore = "requires explicitly provisioned Sway, D-Bus and Linux user namespaces"]
fn headless_desktop_has_a_private_bus_and_compositor() {
    let tmp = tempfile::tempdir().unwrap();
    let source = source(tmp.path());
    let mut profile = profile();
    let sway = PathBuf::from(std::env::var("HARBOR_TEST_SWAY").unwrap())
        .canonicalize()
        .unwrap();
    profile.tools.sway = Some(sway.clone());
    profile.tools.dbus = Some(
        PathBuf::from(std::env::var("HARBOR_TEST_DBUS").unwrap())
            .canonicalize()
            .unwrap(),
    );
    profile.desktop = Desktop::Headless;
    profile.command = vec![
        "bash".into(),
        "-c".into(),
        format!(
            "test -S \"$WAYLAND_DISPLAY\" && test -n \"$DBUS_SESSION_BUS_ADDRESS\" && test ! -e /dev/dri && for sock in \"$XDG_RUNTIME_DIR\"/sway-ipc.*.sock; do '{}' -s \"$sock\" -t get_outputs > \"$HARBOR_ARTIFACTS/outputs.json\"; done && grep -q HEADLESS \"$HARBOR_ARTIFACTS/outputs.json\" && sleep 0.4",
            sway.parent().unwrap().join("swaymsg").display()
        ),
    ];
    let session = Session::open(&profile, &source, &tmp.path().join("state"), "desktop").unwrap();
    let directory = session.directory().to_owned();
    let result = run(
        session,
        RunOptions {
            watch: false,
            parent_wayland: None,
            cancel: Arc::new(AtomicBool::new(false)),
        },
    );
    if !matches!(result, Ok(0)) {
        for log in ["generations/1/preview.log", "runtime/artifacts/1/sway.log"] {
            eprintln!(
                "{log}: {}",
                fs::read_to_string(directory.join(log)).unwrap_or_default()
            );
        }
    }
    assert_eq!(result.unwrap(), 0);
}

#[test]
#[ignore = "requires explicitly provisioned Bubblewrap and Linux user namespaces"]
fn lockfile_mutation_and_build_timeout_fail_closed() {
    let tmp = tempfile::tempdir().unwrap();
    let source = source(tmp.path());
    let mut profile = profile();
    profile.shell_hook.clear();
    profile.build = Some(vec![
        "bash".into(),
        "-c".into(),
        "printf changed > Cargo.lock".into(),
    ]);
    let options = || RunOptions {
        watch: false,
        parent_wayland: None,
        cancel: Arc::new(AtomicBool::new(false)),
    };
    let session = Session::open(&profile, &source, &tmp.path().join("state"), "locks").unwrap();
    assert_eq!(run(session, options()).unwrap(), 1);
    assert_eq!(
        fs::read_to_string(source.join("Cargo.lock")).unwrap(),
        "locked"
    );
    profile.build = Some(vec!["sleep".into(), "20".into()]);
    profile.build_timeout_seconds = 1;
    let session = Session::open(&profile, &source, &tmp.path().join("state"), "timeout").unwrap();
    let start = Instant::now();
    assert_eq!(run(session, options()).unwrap(), 1);
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[test]
#[ignore = "requires explicitly provisioned Rust compiler, linker and Linux namespaces"]
fn cargo_preview_reuses_incremental_artifacts_with_private_mock_services() {
    let tmp = tempfile::tempdir().unwrap();
    let source = source(tmp.path());
    fs::write(source.join("Cargo.toml"), "[package]\nname = \"preview\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[[bin]]\nname = \"preview\"\npath = \"preview.rs\"\n").unwrap();
    fs::write(
        source.join("Cargo.lock"),
        "version = 4\n\n[[package]]\nname = \"preview\"\nversion = \"0.0.0\"\n",
    )
    .unwrap();
    fs::write(
        source.join("preview.rs"),
        include_str!("fixtures/preview.rs"),
    )
    .unwrap();
    let mut profile = profile();
    profile
        .inputs
        .extend(["Cargo.toml".into(), "preview.rs".into()]);
    let path = format!(
        "{}:{}",
        profile.environment["PATH"],
        std::env::var("HARBOR_TEST_RUST_PATH").unwrap()
    );
    profile.environment.insert("PATH".into(), path);
    profile.build = Some(vec![
        "cargo".into(),
        "build".into(),
        "--offline".into(),
        "--locked".into(),
    ]);
    profile.build_timeout_seconds = 15;
    profile.services = vec![vec![
        "/harbor/build/target/debug/preview".into(),
        "mock".into(),
    ]];
    profile.command = vec!["/harbor/build/target/debug/preview".into()];
    profile.ready = Some(vec!["bash".into(), "-c".into(), "test \"$(cat \"$XDG_RUNTIME_DIR/response\")\" = synthetic && test \"$(cat design)\" = \"$(cat \"$XDG_RUNTIME_DIR/version\")\"".into()]);
    let session = Session::open(&profile, &source, &tmp.path().join("state"), "rust").unwrap();
    let directory = session.directory().to_owned();
    let worker = Supervisor::start(session, None);
    wait_for(&directory, |r| r.phase == "running");
    let first = status(&directory).unwrap();
    fs::write(source.join("design"), "second").unwrap();
    wait_for(&directory, |r| r.successful_switches == 2);
    let second = status(&directory).unwrap();
    assert_eq!(first.environment_hash, second.environment_hash);
    assert_ne!(first.preview_source_hash, second.preview_source_hash);
    assert!(
        directory
            .join("build")
            .join(&first.environment_hash)
            .join("target/debug/incremental")
            .is_dir()
    );
    assert_eq!(
        fs::read_to_string(
            directory
                .join("runtime/run")
                .join(second.preview_generation.unwrap().to_string())
                .join("version")
        )
        .unwrap(),
        "second"
    );
    eprintln!(
        "Rust preview cold build: {:?}ms, warm build: {:?}ms; ready launch: {:?}ms",
        first.build_ms, second.build_ms, second.launch_ms
    );
    worker.finish();
}
