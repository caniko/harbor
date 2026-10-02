use harbor_sandbox::{Desktop, Profile, RunOptions, Session, Tools, run, status};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

pub fn profile() -> Profile {
    let get = |name| {
        PathBuf::from(std::env::var(name).expect("explicit sandbox test tools required"))
            .canonicalize()
            .unwrap()
    };
    Profile {
        schema_version: 1,
        name: "fixture".into(),
        tools: Tools {
            bwrap: get("HARBOR_TEST_BWRAP"),
            bash: get("HARBOR_TEST_BASH"),
            dbus: None,
            sway: None,
        },
        environment: BTreeMap::from([("PATH".into(), std::env::var("HARBOR_TEST_PATH").unwrap())]),
        shell_hook: "export DERIVED_DATA=\"$XDG_DATA_HOME/fixture\"".into(),
        inputs: vec!["design".into(), "Cargo.lock".into()],
        design_copies: vec!["design".into()],
        command: vec![
            "bash".into(),
            "-c".into(),
            "while :; do sleep 0.1; done".into(),
        ],
        services: vec![],
        build: None,
        ready: None,
        desktop: Desktop::None,
        network: false,
        gpu: false,
        cargo_config: None,
        poll_ms: 20,
        debounce_ms: 40,
        build_timeout_seconds: 3,
        ready_timeout_seconds: 3,
    }
}

pub fn source(root: &Path) -> PathBuf {
    let path = root.join("source");
    fs::create_dir(&path).unwrap();
    fs::write(path.join("design"), "good").unwrap();
    fs::write(path.join("Cargo.lock"), "locked").unwrap();
    path
}

pub fn wait_for(directory: &Path, predicate: impl Fn(&harbor_sandbox::Receipt) -> bool) {
    let start = Instant::now();
    loop {
        if let Ok(receipt) = status(directory) {
            if predicate(&receipt) {
                return;
            }
            assert!(receipt.active, "supervisor exited: {receipt:?}");
        }
        if start.elapsed() >= Duration::from_secs(20) {
            eprintln!("last receipt: {:?}", status(directory));
            for entry in fs::read_dir(directory.join("generations")).unwrap() {
                for name in ["build.log", "preview.log"] {
                    let path = entry.as_ref().unwrap().path().join(name);
                    eprintln!(
                        "{}: {}",
                        path.display(),
                        fs::read_to_string(&path).unwrap_or_default()
                    );
                }
            }
            for entry in fs::read_dir(directory.join("runtime/artifacts")).unwrap() {
                let path = entry.unwrap().path().join("sway.log");
                eprintln!(
                    "{}: {}",
                    path.display(),
                    fs::read_to_string(&path).unwrap_or_default()
                );
            }
            panic!("timed out: {}", directory.display());
        }
        thread::sleep(Duration::from_millis(20));
    }
}

pub struct Supervisor {
    cancel: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<anyhow::Result<i32>>>,
}

impl Supervisor {
    pub fn start(session: Session, parent_wayland: Option<PathBuf>) -> Self {
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let worker = thread::spawn(move || {
            run(
                session,
                RunOptions {
                    watch: true,
                    parent_wayland,
                    cancel: worker_cancel,
                },
            )
        });
        Self {
            cancel,
            worker: Some(worker),
        }
    }

    pub fn finish(mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        assert_eq!(self.worker.take().unwrap().join().unwrap().unwrap(), 0);
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
