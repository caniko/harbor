//! Nested software-desktop acceptance against a test-owned parent compositor.
#![cfg(target_os = "linux")]
mod common;
use common::{Supervisor, profile, source, wait_for};
use harbor_sandbox::{Desktop, Session, status, stop};
use std::fs;
use std::os::unix::fs::FileTypeExt;
use std::path::PathBuf;

#[test]
#[ignore = "requires provisioned Sway and Linux namespaces; no live host display"]
fn nested_preview_uses_only_the_declared_parent_and_stops_independently() {
    let tmp = tempfile::tempdir().unwrap();
    let source = source(tmp.path());
    let mut parent = profile();
    parent.tools.sway = Some(
        PathBuf::from(std::env::var("HARBOR_TEST_SWAY").unwrap())
            .canonicalize()
            .unwrap(),
    );
    parent.tools.dbus = Some(
        PathBuf::from(std::env::var("HARBOR_TEST_DBUS").unwrap())
            .canonicalize()
            .unwrap(),
    );
    parent.desktop = Desktop::Headless;
    let state = tmp.path().join("state");
    let session = Session::open(&parent, &source, &state, "parent").unwrap();
    let parent_dir = session.directory().to_owned();
    let outer = Supervisor::start(session, None);
    wait_for(&parent_dir, |r| r.phase == "running");
    let run = parent_dir.join("runtime/run/1");
    let socket = fs::read_dir(&run)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("wayland-")
                && fs::metadata(p).unwrap().file_type().is_socket()
        })
        .unwrap();
    let mut nested = parent.clone();
    nested.desktop = Desktop::Nested;
    nested.ready_timeout_seconds = 5;
    nested.ready = Some(vec!["bash".into(), "-c".into(), "test -S \"$WAYLAND_DISPLAY\" && test \"$WAYLAND_DISPLAY\" != /harbor/parent/wayland && test ! -e /dev/dri && test -n \"$DBUS_SESSION_BUS_ADDRESS\"".into()]);
    let session = Session::open(&nested, &source, &state, "nested").unwrap();
    let nested_dir = session.directory().to_owned();
    let inner = Supervisor::start(session, Some(socket));
    wait_for(&nested_dir, |r| r.phase == "running");
    stop(&nested_dir).unwrap();
    assert!(status(&parent_dir).unwrap().active);
    inner.finish();
    outer.finish();
}
