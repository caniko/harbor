use crate::session::{atomic_json, lock_file, private_dir};
use crate::{Desktop, Session, Snapshot};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Write;
use std::fs::{self, File};
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub schema_version: u32,
    pub profile: String,
    pub variant: String,
    pub run_id: String,
    pub source: PathBuf,
    pub environment_hash: String,
    pub active: bool,
    pub phase: String,
    pub generation: u64,
    pub preview_generation: Option<u64>,
    pub source_hash: Option<String>,
    pub preview_source_hash: Option<String>,
    pub build_ms: Option<u64>,
    pub launch_ms: Option<u64>,
    pub successful_switches: u64,
    pub last_error: Option<String>,
    pub network: bool,
    pub gpu: bool,
    pub desktop: Desktop,
}

pub struct RunOptions {
    pub watch: bool,
    /// Resolved by the caller from its current graphical session, only for Nested.
    pub parent_wayland: Option<PathBuf>,
    /// Set on SIGINT/SIGTERM by the CLI; embedders can cancel without signals.
    pub cancel: Arc<AtomicBool>,
}

struct OwnedChild(Child, bool);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.1 {
            if let Some(input) = self.0.stdin.as_mut() {
                let _ = std::io::Write::write_all(input, b"stop\n");
            }
            let started = Instant::now();
            while started.elapsed() < Duration::from_millis(1500) {
                if self.0.try_wait().is_ok_and(|exit| exit.is_some()) {
                    return;
                }
                thread::sleep(Duration::from_millis(20));
            }
        }
        // No stored PID is ever signalled. Bubblewrap's private PID namespace and
        // die-with-parent contract contain descendants when its monitor exits.
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub fn run(session: Session, options: RunOptions) -> Result<i32> {
    let run_id = format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    );
    let mut receipt = Receipt {
        schema_version: 1,
        profile: session.profile.name.clone(),
        variant: session.variant.clone(),
        run_id,
        source: session.source.clone(),
        environment_hash: session.environment_hash.clone(),
        active: true,
        phase: "starting".into(),
        generation: 0,
        preview_generation: None,
        source_hash: None,
        preview_source_hash: None,
        build_ms: None,
        launch_ms: None,
        successful_switches: 0,
        last_error: None,
        network: session.profile.network,
        gpu: session.profile.gpu,
        desktop: session.profile.desktop,
    };
    let path = session.directory.join("receipt.json");
    atomic_json(&path, &receipt)?;
    let result = supervise(&session, &options, &mut receipt);
    receipt.active = false;
    receipt.phase = if matches!(result, Ok(0)) {
        "stopped"
    } else {
        "failed"
    }
    .into();
    if let Err(error) = &result {
        receipt.last_error = Some(format!("{error:#}"));
    }
    atomic_json(&path, &receipt)?;
    // Ownership keeps the kernel lock for the full worker lifetime and releases
    // it only after children and the terminal receipt are complete.
    drop(session);
    drop(options);
    result
}

fn supervise(session: &Session, options: &RunOptions, receipt: &mut Receipt) -> Result<i32> {
    let poll = Duration::from_millis(session.profile.poll_ms);
    let mut preview: Option<OwnedChild> = None;
    let mut observed: Option<String> = None;
    let mut cached_source: Option<(String, String)> = None;
    let mut settled_at = Instant::now();
    let mut attempted: Option<String> = None;
    let mut retry_at: Option<Instant> = None;
    loop {
        if cancelled(session, options, &receipt.run_id) {
            return Ok(0);
        }
        let hash = match scan_source(session, &mut cached_source) {
            Ok(hash) => hash,
            Err(error) if options.watch => {
                receipt.last_error = Some(format!("source scan: {error:#}"));
                save(session, receipt)?;
                thread::sleep(poll);
                continue;
            }
            Err(error) => return Err(error),
        };
        if observed.as_ref() != Some(&hash) {
            observed = Some(hash.clone());
            settled_at = Instant::now();
            retry_at = None;
        }
        let needs_build = attempted.as_ref() != Some(&hash);
        if needs_build
            && retry_at.is_none_or(|deadline| Instant::now() >= deadline)
            && (attempted.is_none()
                || settled_at.elapsed() >= Duration::from_millis(session.profile.debounce_ms))
        {
            // Generation numbers persist across supervisor restarts; old evidence is retained.
            receipt.generation = next_generation(session, receipt.generation);
            receipt.phase = "building".into();
            receipt.last_error = None;
            save(session, receipt)?;
            let Some(snapshot) = build_generation(session, options, receipt, &hash)? else {
                if receipt.phase == "build-failed" {
                    attempted = Some(hash);
                }
                if !options.watch {
                    return Ok(1);
                }
                thread::sleep(poll);
                continue;
            };
            attempted = Some(hash.clone());
            if cancelled(session, options, &receipt.run_id) {
                return Ok(0);
            }
            receipt.phase = "starting-preview".into();
            save(session, receipt)?;
            if let Some(candidate) = ready_candidate(session, options, receipt, &snapshot)? {
                // Drop only the previous namespace AFTER the candidate passed readiness.
                preview = Some(candidate);
                receipt.preview_generation = Some(receipt.generation);
                receipt.preview_source_hash = Some(hash);
                receipt.successful_switches += 1;
                receipt.phase = "running".into();
                save(session, receipt)?;
            } else {
                receipt.phase = "preview-failed".into();
                receipt.last_error = Some(
                    "candidate exited or readiness timed out; previous preview retained".into(),
                );
                save(session, receipt)?;
                if !options.watch {
                    return Ok(1);
                }
                attempted = None;
                retry_at = Some(Instant::now() + poll.max(Duration::from_secs(1)));
            }
        }
        if let Some(child) = &mut preview
            && let Some(exit) = child.0.try_wait()?
        {
            if !options.watch {
                return Ok(exit.code().unwrap_or(1));
            }
            receipt.phase = "preview-exited".into();
            receipt.last_error = Some(format!("preview exited: {exit}"));
            save(session, receipt)?;
            preview = None;
            attempted = None;
            retry_at = Some(Instant::now() + poll.max(Duration::from_secs(1)));
        }
        thread::sleep(poll);
    }
}

fn build_generation(
    session: &Session,
    options: &RunOptions,
    receipt: &mut Receipt,
    hash: &str,
) -> Result<Option<Snapshot>> {
    let started = Instant::now();
    let snapshot = match session.snapshot(receipt.generation) {
        Ok(snapshot) => snapshot,
        Err(error) if options.watch => {
            receipt.phase = "source-error".into();
            receipt.last_error = Some(format!("source snapshot: {error:#}"));
            save(session, receipt)?;
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    receipt.source_hash = Some(snapshot.source_hash.clone());
    // Copy hashes must match both the settled input and its post-copy content.
    // Mid-copy edits are retried without retiring the previous preview.
    if snapshot.source_hash != hash || !session.source_hash().is_ok_and(|current| current == hash) {
        receipt.phase = "source-changed".into();
        save(session, receipt)?;
        return Ok(None);
    }
    prepare_artifacts(session, &snapshot, receipt.generation)?;
    let built = build_snapshot(session, options, receipt, &snapshot)?;
    receipt.build_ms = Some(milliseconds(started.elapsed()));
    let lock_result = snapshot.verify_locks();
    if !built || lock_result.is_err() {
        receipt.phase = "build-failed".into();
        receipt.last_error = Some(lock_result.err().map_or_else(
            || "build failed, timed out or was cancelled; see generation build.log".into(),
            |e| e.to_string(),
        ));
        save(session, receipt)?;
        return Ok(None);
    }
    Ok(Some(snapshot))
}

fn build_snapshot(
    session: &Session,
    options: &RunOptions,
    receipt: &Receipt,
    snapshot: &Snapshot,
) -> Result<bool> {
    let Some(build) = &session.profile.build else {
        return Ok(true);
    };
    let mut child = spawn(session, snapshot, receipt.generation, build, false, options)?;
    wait_bounded(
        &mut child,
        session,
        options,
        &receipt.run_id,
        Duration::from_secs(session.profile.build_timeout_seconds),
    )
}

fn scan_source(session: &Session, cached: &mut Option<(String, String)>) -> Result<String> {
    let current = session.source_stamp()?;
    if let Some((stamp, hash)) = cached
        && *stamp == current
    {
        return Ok(hash.clone());
    }
    let hash = session.source_hash()?;
    *cached = Some((current, hash.clone()));
    Ok(hash)
}

fn ready_candidate(
    session: &Session,
    options: &RunOptions,
    receipt: &mut Receipt,
    snapshot: &Snapshot,
) -> Result<Option<OwnedChild>> {
    let launched = Instant::now();
    let mut candidate = spawn(
        session,
        snapshot,
        receipt.generation,
        &session.profile.command,
        true,
        options,
    )?;
    let ready = ready_path(session, receipt.generation);
    let deadline = Duration::from_secs(session.profile.ready_timeout_seconds);
    while !ready.exists()
        && launched.elapsed() < deadline
        && !cancelled(session, options, &receipt.run_id)
    {
        if candidate.0.try_wait()?.is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(session.profile.poll_ms));
    }
    if !ready.exists()
        || candidate.0.try_wait()?.is_some()
        || cancelled(session, options, &receipt.run_id)
    {
        return Ok(None);
    }
    receipt.launch_ms = Some(milliseconds(launched.elapsed()));
    Ok(Some(candidate))
}

fn next_generation(session: &Session, after: u64) -> u64 {
    let mut next = after + 1;
    while session
        .directory
        .join("generations")
        .join(next.to_string())
        .exists()
    {
        next += 1;
    }
    next
}

fn save(session: &Session, receipt: &Receipt) -> Result<()> {
    atomic_json(&session.directory.join("receipt.json"), receipt)
}

fn cancelled(session: &Session, options: &RunOptions, run_id: &str) -> bool {
    options.cancel.load(Ordering::Relaxed)
        || fs::read_to_string(session.directory.join("stop.request"))
            .is_ok_and(|value| value == run_id)
}

fn wait_bounded(
    child: &mut OwnedChild,
    session: &Session,
    options: &RunOptions,
    run_id: &str,
    timeout: Duration,
) -> Result<bool> {
    let start = Instant::now();
    loop {
        if let Some(status) = child.0.try_wait()? {
            return Ok(status.success());
        }
        if start.elapsed() >= timeout || cancelled(session, options, run_id) {
            return Ok(false);
        }
        thread::sleep(Duration::from_millis(session.profile.poll_ms));
    }
}

fn milliseconds(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn ready_path(session: &Session, generation: u64) -> PathBuf {
    session
        .runtime
        .join("run")
        .join(generation.to_string())
        .join("ready")
}

fn spawn(
    session: &Session,
    snapshot: &Snapshot,
    generation: u64,
    command: &[String],
    preview: bool,
    options: &RunOptions,
) -> Result<OwnedChild> {
    let generation_dir = snapshot
        .workspace
        .parent()
        .context("generation directory")?;
    let run_dir = session.runtime.join("run").join(generation.to_string());
    private_dir(&run_dir)?;
    let script = generation_dir.join(if preview { "preview.sh" } else { "build.sh" });
    fs::write(&script, script_text(session, generation, command, preview))?;
    let mut child =
        namespace_command(session, snapshot, generation_dir, &script, preview, options)?;
    let mut environment = session.profile.environment.clone();
    environment.extend(managed_environment(generation));
    for (key, value) in environment {
        child.arg("--setenv").arg(key).arg(value);
    }
    child.args(["--chdir", "/workspace", "--"]);
    if preview && let Some(dbus) = &session.profile.tools.dbus {
        child
            .arg(dbus)
            .arg("--dbus-daemon")
            .arg(
                dbus.parent()
                    .context("D-Bus tool parent")?
                    .join("dbus-daemon"),
            )
            .args(["--config-file", "/harbor/dbus.conf", "--"]);
    }
    child
        .arg(&session.profile.tools.bash)
        .args(["--noprofile", "--norc", "/harbor/entry.sh"]);
    let log = File::create(generation_dir.join(if preview { "preview.log" } else { "build.log" }))?;
    child
        .stdin(if preview {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(log.try_clone()?)
        .stderr(log);
    Ok(OwnedChild(
        child
            .spawn()
            .context("start Bubblewrap sandbox (required; no unconfined fallback)")?,
        preview,
    ))
}

fn prepare_artifacts(session: &Session, snapshot: &Snapshot, generation: u64) -> Result<()> {
    let artifacts = session
        .runtime
        .join("artifacts")
        .join(generation.to_string());
    private_dir(&artifacts)?;
    private_dir(&session.runtime.join("run").join(generation.to_string()))?;
    // Prepare copies BEFORE any command can modify this generation. Neither
    // a build nor an old preview can redirect these host copies through symlinks.
    for copy in &session.profile.design_copies {
        let original = snapshot.workspace.join(copy);
        ensure!(
            fs::symlink_metadata(&original)?.is_file(),
            "design copy must be a regular input file: {}",
            copy.display()
        );
        let destination = artifacts.join("designs").join(copy);
        private_dir(destination.parent().context("design destination")?)?;
        fs::copy(original, destination)?;
    }
    Ok(())
}

fn namespace_command(
    session: &Session,
    snapshot: &Snapshot,
    generation_dir: &Path,
    script: &Path,
    preview: bool,
    options: &RunOptions,
) -> Result<Command> {
    let mut child = Command::new(&session.profile.tools.bwrap);
    child
        .env_clear()
        .args(["--die-with-parent", "--new-session", "--unshare-all"]);
    if session.profile.network {
        child.arg("--share-net");
    }
    child.args([
        "--ro-bind",
        "/nix/store",
        "/nix/store",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        "--tmpfs",
        "/tmp",
        "--dir",
        "/etc",
    ]);
    // Minimal host-independent NSS; network capability additionally needs DNS.
    let passwd = generation_dir.join("passwd");
    fs::write(
        &passwd,
        format!(
            "sandbox:x:{}:{}::/harbor/state/home:/bin/bash\n",
            fs::metadata("/proc/self")?.uid(),
            fs::metadata("/proc/self")?.gid()
        ),
    )?;
    child.arg("--ro-bind").arg(&passwd).arg("/etc/passwd");
    if preview && session.profile.tools.dbus.is_some() {
        let bus_config = generation_dir.join("dbus.conf");
        fs::write(
            &bus_config,
            "<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><auth>EXTERNAL</auth><standard_session_servicedirs/><policy context=\"default\"><allow send_destination=\"*\"/><allow receive_sender=\"*\"/><allow own=\"*\"/></policy></busconfig>",
        )?;
        child
            .arg("--ro-bind")
            .arg(bus_config)
            .arg("/harbor/dbus.conf");
    }
    if session.profile.network {
        child.args(["--ro-bind", "/etc/resolv.conf", "/etc/resolv.conf"]);
    }
    child
        .arg("--symlink")
        .arg(&session.profile.tools.bash)
        .arg("/bin/bash");
    child
        .arg(if preview { "--ro-bind" } else { "--bind" })
        .arg(&snapshot.workspace)
        .arg("/workspace");
    child.arg("--bind").arg(&session.build).arg("/harbor/build");
    // Expose mount leaves, never the host ancestors used to allocate subsequent
    // generations. A live preview cannot redirect supervisor writes via symlinks.
    for name in ["home", "config", "data", "cache", "state", "tmp"] {
        child
            .arg("--bind")
            .arg(session.runtime.join(name))
            .arg(format!("/harbor/state/{name}"));
    }
    let generation = snapshot
        .workspace
        .parent()
        .and_then(Path::file_name)
        .context("generation name")?;
    for name in ["run", "artifacts"] {
        child
            .arg("--bind")
            .arg(session.runtime.join(name).join(generation))
            .arg(Path::new("/harbor/state").join(name).join(generation));
    }
    child.arg("--ro-bind").arg(script).arg("/harbor/entry.sh");
    if session.profile.gpu {
        child.args(["--dev-bind", "/dev/dri", "/dev/dri"]);
    }
    if preview && session.profile.desktop == Desktop::Nested {
        let socket = options
            .parent_wayland
            .as_ref()
            .context("nested desktop needs the current WAYLAND_DISPLAY socket")?;
        ensure!(
            socket.is_absolute() && fs::metadata(socket)?.file_type().is_socket(),
            "parent Wayland endpoint is not an absolute socket"
        );
        child
            .arg("--ro-bind")
            .arg(socket)
            .arg("/harbor/parent/wayland");
    }
    Ok(child)
}

use std::os::unix::fs::MetadataExt;

fn managed_environment(generation: u64) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    for (key, suffix) in [
        ("HOME", "home"),
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_STATE_HOME", "state"),
        ("TMPDIR", "tmp"),
    ] {
        result.insert(key.into(), format!("/harbor/state/{suffix}"));
    }
    result.insert(
        "XDG_RUNTIME_DIR".into(),
        format!("/harbor/state/run/{generation}"),
    );
    result.insert(
        "HARBOR_ARTIFACTS".into(),
        format!("/harbor/state/artifacts/{generation}"),
    );
    result.insert(
        "HARBOR_BROWSER_PROFILE".into(),
        format!("/harbor/state/run/{generation}/browser"),
    );
    result.insert(
        "BLENDER_USER_CONFIG".into(),
        format!("/harbor/state/run/{generation}/blender"),
    );
    result.insert("HARBOR_WORKSPACE".into(), "/workspace".into());
    result.insert("CARGO_TARGET_DIR".into(), "/harbor/build/target".into());
    result.insert("CARGO_HOME".into(), "/harbor/build/cargo".into());
    result.insert("LC_ALL".into(), "C.UTF-8".into());
    result
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
fn argv(command: &[String]) -> String {
    command
        .iter()
        .map(|s| quote(s))
        .collect::<Vec<_>>()
        .join(" ")
}

fn script_text(session: &Session, generation: u64, command: &[String], preview: bool) -> String {
    let mut text = String::from("set -euo pipefail\numask 077\nharbor_shell_hook() {\n:\n");
    text.push_str(&session.profile.shell_hook);
    text.push_str("\n}\nharbor_shell_hook\n");
    // Hooks derive paths using private state; restore owned variables if a hook altered
    // them. Profile hooks themselves are trusted, like devShell shellHooks.
    for (key, value) in managed_environment(generation) {
        let _ = writeln!(text, "export {key}={}", quote(&value));
    }
    if !preview {
        let _ = writeln!(text, "exec {}", argv(command));
        return text;
    }
    text.push_str("service_pids=()\ncheck_services() {\n for pid in \"${service_pids[@]}\"; do\n  kill -0 \"$pid\" 2>/dev/null || { printf 'private service exited: %s\\n' \"$pid\" >&2; return 1; }\n done\n}\n");
    if session.profile.desktop != Desktop::None {
        text.push_str("printf '%s\\n' 'output * resolution 1280x800' 'seat seat0 fallback true' 'font monospace 10' > \"$XDG_RUNTIME_DIR/sway.conf\"\n");
        let backend = if session.profile.desktop == Desktop::Headless {
            "WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_HEADLESS_OUTPUTS=1"
        } else if session.profile.gpu {
            "WAYLAND_DISPLAY=/harbor/parent/wayland WLR_BACKENDS=wayland WLR_RENDERER=gles2"
        } else {
            "WAYLAND_DISPLAY=/harbor/parent/wayland WLR_BACKENDS=wayland WLR_RENDERER=pixman"
        };
        let _ = writeln!(
            text,
            "{backend} WLR_LIBINPUT_NO_DEVICES=1 {} --unsupported-gpu -c \"$XDG_RUNTIME_DIR/sway.conf\" >\"$HARBOR_ARTIFACTS/sway.log\" 2>&1 &\nsway_pid=$!",
            quote(
                &session
                    .profile
                    .tools
                    .sway
                    .as_ref()
                    .expect("validated desktop tools")
                    .to_string_lossy()
            )
        );
        text.push_str("while :; do\n kill -0 \"$sway_pid\" || exit 1\n for socket in \"$XDG_RUNTIME_DIR\"/wayland-*; do\n  if [ -S \"$socket\" ]; then export WAYLAND_DISPLAY=\"$socket\"; break 2; fi\n done\n sleep 0.05\ndone\n");
        text.push_str("service_pids+=(\"$sway_pid\")\n");
    }
    for service in &session.profile.services {
        let _ = writeln!(text, "{} &\nservice_pids+=(\"$!\")", argv(service));
    }
    let _ = writeln!(text, "{} &\npreview_pid=$!", argv(command));
    if let Some(ready) = &session.profile.ready {
        let _ = writeln!(
            text,
            "until {}; do\n check_services || exit 1\n kill -0 \"$preview_pid\" || exit 1\n sleep 0.05\ndone",
            argv(ready)
        );
    } else {
        text.push_str("sleep 0.2\n");
    }
    text.push_str("check_services || exit 1\nkill -0 \"$preview_pid\"\nprintf ready > \"$XDG_RUNTIME_DIR/ready\"\nwhile kill -0 \"$preview_pid\" 2>/dev/null; do\n check_services || exit 1\n if read -r -t 0.05 control && [ \"$control\" = stop ]; then\n  kill -TERM \"$preview_pid\" 2>/dev/null || true\n  wait \"$preview_pid\" || true\n  exit 0\n fi\ndone\nwait \"$preview_pid\"\n");
    text
}

pub fn status(directory: &Path) -> Result<Receipt> {
    ensure!(
        directory.is_dir() && directory.join("supervisor.lock").is_file(),
        "sandbox session does not exist: {}",
        directory.display()
    );
    private_dir(directory)?;
    let lock = lock_file(directory)?;
    let active = match lock.try_lock() {
        Ok(()) => false,
        Err(std::fs::TryLockError::WouldBlock) => true,
        Err(error) => return Err(error.into()),
    };
    let mut receipt: Receipt = serde_json::from_slice(&fs::read(directory.join("receipt.json"))?)?;
    receipt.active = active;
    if !active && receipt.phase != "stopped" && receipt.phase != "failed" {
        receipt.phase = "interrupted".into();
    }
    Ok(receipt)
}

/// Ask the owning supervisor to stop. Never signal a persisted PID or unlink a lock.
pub fn stop(directory: &Path) -> Result<()> {
    let receipt = status(directory)?;
    if !receipt.active {
        return Ok(());
    }
    let request = directory.join("stop.request");
    // Cooperative control files are outside all writable sandbox mounts.
    fs::write(&request, receipt.run_id)?;
    let started = Instant::now();
    while status(directory)?.active {
        ensure!(
            started.elapsed() < Duration::from_secs(10),
            "supervisor did not stop within 10 seconds"
        );
        thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}
