use harbor_cad::{contracts::invalid, lifecycle::NativeProcess};
use std::{path::Path, process::Command, time::Duration};

fn running(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|s| {
        s.rsplit_once(") ")
            .is_some_and(|(_, rest)| !rest.starts_with('Z'))
    })
}

fn fixture(root: &Path, exit_early: bool) -> NativeProcess {
    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg(if exit_early {
            "sleep 30 & echo $! > child.pid; exit 7"
        } else {
            "sleep 30 & echo $! > child.pid; wait"
        })
        .current_dir(root);
    NativeProcess::spawn(command).unwrap()
}

#[test]
fn native_monitor_error_timeout_and_exit_clean_up_owned_descendants() {
    for mode in ["monitor", "timeout", "exit"] {
        let temporary = tempfile::tempdir().unwrap();
        let mut child = fixture(temporary.path(), mode == "exit");
        let marker = temporary.path().join("child.pid");
        let result = child.wait(Duration::from_millis(300), || {
            if mode == "monitor" && marker.exists() {
                return Err(invalid("output symlink fixture"));
            }
            Ok(())
        });
        if mode == "exit" {
            assert_eq!(result.unwrap().code(), Some(7));
        } else {
            assert!(result.is_err());
        }
        let pid: u32 = std::fs::read_to_string(marker)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert!(!running(pid), "owned descendant still running after {mode}");
    }
}

#[test]
fn dropping_native_process_does_not_signal_an_unrelated_group() {
    let mut unrelated = Command::new("sleep").arg("30").spawn().unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let native = fixture(temporary.path(), false);
    drop(native);
    assert!(unrelated.try_wait().unwrap().is_none());
    unrelated.kill().unwrap();
    unrelated.wait().unwrap();
}
