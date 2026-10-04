use crate::{Error, Result, contracts::*, devices, science, storage::*};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    os::unix::{
        fs::{OpenOptionsExt, PermissionsExt},
        net::{UnixListener, UnixStream},
        process::CommandExt,
    },
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Serialize, Deserialize, Debug)]
pub struct Response {
    pub protocol_version: u32,
    pub request_id: String,
    pub ok: bool,
    pub data: Option<serde_json::Value>,
    pub error: Option<serde_json::Value>,
}
fn reply(request_id: String, result: Result<serde_json::Value>) -> Response {
    match result {
        Ok(data) => Response {
            protocol_version: 1,
            request_id,
            ok: true,
            data: Some(data),
            error: None,
        },
        Err(error) => Response {
            protocol_version: 1,
            request_id,
            ok: false,
            data: None,
            error: Some(serde_json::to_value(error.diagnostic()).unwrap_or_default()),
        },
    }
}
pub fn load_profile(path: &Path) -> Result<HostExecutionProfile> {
    let profile: HostExecutionProfile = serde_json::from_slice(&read_bounded(path, MAX_MESSAGE)?)?;
    if profile.schema_version != 1
        || profile.threads == 0
        || profile.threads > 128
        || profile.max_ram_bytes == 0
        || profile.max_ram_bytes > i64::MAX as u64
        || profile.max_disk_bytes == 0
        || profile.timeout_seconds == 0
        || profile.timeout_seconds > 86400
        || !["systemd", "foreground"].contains(&profile.service_mode.as_str())
        || !["ci", "prototype", "production", "research"].contains(&profile.policy.as_str())
    {
        return Err(invalid("host profile limits/version/policy"));
    }
    if profile.service_mode == "foreground" && profile.policy != "ci" {
        return Err(invalid("foreground is explicitly weaker CI-only execution"));
    }
    if !Path::new(&profile.allowed_input_root).is_absolute() {
        return Err(invalid("absolute input root required"));
    }
    Ok(profile)
}
pub fn read_bounded(path: &Path, maximum: u64) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    fs::File::open(path)?
        .take(maximum + 1)
        .read_to_end(&mut data)?;
    if data.len() as u64 > maximum {
        return Err(invalid("file/request size limit"));
    }
    Ok(data)
}
fn systemctl(args: &[&str]) -> Result<std::process::Output> {
    Ok(
        Command::new(option_env!("HARBOR_CAD_SYSTEMCTL").unwrap_or("systemctl"))
            .arg("--user")
            .args(args)
            .stdin(Stdio::null())
            .output()?,
    )
}
fn unit_info(unit: &str) -> Result<BTreeMap<String, String>> {
    let output = systemctl(&[
        "show",
        unit,
        "--property=LoadState,ActiveState,InvocationID,ExecMainStatus,Result",
    ])?;
    if !output.status.success() {
        return Err(Error::Resource("systemd user manager unavailable".into()));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|l| l.split_once('=').map(|(k, v)| (k.into(), v.into())))
        .collect())
}
fn check_plan(plan: &ExecutionPlan, profile: &HostExecutionProfile) -> Result<()> {
    plan.validate()?;
    if plan.policy != profile.policy {
        return Err(invalid("plan policy differs from effective host profile"));
    }
    if plan
        .stages
        .iter()
        .any(|s| s.ram_bytes > profile.max_ram_bytes)
        || plan.observation.max_artifact_bytes > profile.max_disk_bytes
    {
        return Err(Error::Resource(
            "approved plan exceeds host RAM/disk admission budget; scientific parameters preserved"
                .into(),
        ));
    }
    for stage in &plan.stages {
        if let Some(selection) = &stage.selection {
            // Runtime adapters correlate UUIDs themselves; never treat a Mesa selector as CUDA identity.
            let inventory = devices::inventory()?;
            devices::resolve(
                &inventory,
                selection.role,
                &selection.backend,
                Some(&selection.pci),
            )?;
        }
        if !matches!(
            stage.operation,
            StageOperation::ChannelReference | StageOperation::Bundle
        ) {
            if profile.service_mode != "systemd" {
                return Err(Error::Unqualified(
                    "native stages require systemd cgroup ownership".into(),
                ));
            }
            let runtime = profile.native_runtime.as_ref().ok_or_else(|| {
                Error::Unqualified("isolated native runtime not configured".into())
            })?;
            let native = NativeRuntime::load(Path::new(runtime))?;
            native.executable(&stage.operation)?;
            if matches!(stage.operation, StageOperation::Openlb) {
                let backend = stage
                    .selection
                    .as_ref()
                    .map_or("cpu", |s| s.backend.as_str());
                if native.openlb_backend != backend {
                    return Err(Error::Unqualified(
                        "packaged OpenLB backend differs from approved stage; no fallback".into(),
                    ));
                }
            }
        }
    }
    Ok(())
}
fn launch(
    store: &Store,
    profile: &HostExecutionProfile,
    profile_path: &Path,
    id: &str,
) -> Result<()> {
    let plan = store.plan(id)?;
    check_plan(&plan, profile)?;
    if !store.transition(id, "queued", "starting", None)? {
        return Ok(());
    }
    let exe = std::env::current_exe()?;
    let root = fs::canonicalize(&store.root)?;
    let profile_path = fs::canonicalize(profile_path)?;
    if profile.service_mode == "foreground" {
        let mut cmd = Command::new(exe);
        cmd.args(["run-job", "--state"])
            .arg(&root)
            .arg("--profile")
            .arg(&profile_path)
            .arg(id)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // No native grandchildren in CI foreground mode. Do not reconcile by saved PIDs.
        unsafe {
            cmd.pre_exec(|| {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = cmd.spawn()?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    } else {
        let job = store.job(id)?;
        let output = Command::new(option_env!("HARBOR_CAD_SYSTEMD_RUN").unwrap_or("systemd-run"))
            .args(["--user", "--quiet", "--service-type=exec", "--unit"])
            .arg(&job.unit)
            .arg("--property=KillMode=control-group")
            .arg("--property=TimeoutStopSec=10s")
            .arg("--property=SendSIGKILL=yes")
            .arg("--property=Restart=no")
            .arg(format!("--property=MemoryMax={}", profile.max_ram_bytes))
            .arg("--property=TasksMax=128")
            .arg(format!(
                "--property=RuntimeMaxSec={}",
                profile.timeout_seconds
            ))
            .arg("--property=NoNewPrivileges=yes")
            .arg("--property=UMask=0077")
            .arg(format!("--property=CPUQuota={}00%", profile.threads))
            .arg(exe)
            .arg("run-job")
            .arg("--state")
            .arg(root)
            .arg("--profile")
            .arg(profile_path)
            .arg(id)
            .stdin(Stdio::null())
            .output()?;
        if !output.status.success() {
            store.finish(id, 1, Some("systemd launch failed"))?;
            return Err(Error::Resource(
                String::from_utf8_lossy(&output.stderr).trim().into(),
            ));
        }
    }
    store.event(id, "launched", &profile.service_mode)?;
    Ok(())
}
fn reconcile(
    store: &Store,
    profile: &HostExecutionProfile,
    profile_path: &Path,
    startup: bool,
) -> Result<()> {
    for id in store.active()? {
        let job = store.job(&id)?;
        if job.state == "queued" {
            if let Err(e) = launch(store, profile, profile_path, &id) {
                store.transition(&id, "queued", "failed", Some(&e.to_string()))?;
                store.transition(&id, "starting", "failed", Some(&e.to_string()))?;
                store.event(&id, "launch_rejected", &e.to_string())?;
            }
        } else if profile.service_mode == "foreground" {
            if startup {
                store.transition(
                    &id,
                    &job.state,
                    "interrupted",
                    Some("CI foreground worker restart; no solver resume guarantee"),
                )?;
            }
        } else {
            let info = unit_info(&job.unit)?;
            let observed = info.get("InvocationID").map(String::as_str).unwrap_or("");
            if job
                .invocation_id
                .as_deref()
                .is_some_and(|saved| !observed.is_empty() && observed != saved)
            {
                store.transition(
                    &id,
                    &job.state,
                    "interrupted",
                    Some("unit invocation identity changed; refusing attachment/cancellation"),
                )?;
            } else if info
                .get("ActiveState")
                .is_some_and(|s| s == "inactive" || s == "failed")
                || info.get("LoadState").is_some_and(|s| s == "not-found")
            {
                let code = info
                    .get("ExecMainStatus")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(1);
                store.finish(
                    &id,
                    if code == 0 { 1 } else { code },
                    Some("job ended without durable successful exit receipt"),
                )?;
            }
        }
    }
    Ok(())
}
fn dispatch(
    store: &Store,
    profile: &HostExecutionProfile,
    op: Operation,
) -> Result<serde_json::Value> {
    match op {
        Operation::Doctor => doctor(),
        Operation::BackendList => Ok(backends()),
        Operation::Validate { case } => {
            case.validate()?;
            Ok(
                serde_json::json!({"valid":true,"science_id":case.science_id()?,"physical_validation":"unqualified"}),
            )
        }
        Operation::Plan { case } => Ok(serde_json::to_value(ExecutionPlan::reference(*case)?)?),
        Operation::PlanOpenlbReference { case } => Ok(serde_json::to_value(
            ExecutionPlan::openlb_reference(*case, profile.policy.clone())?,
        )?),
        Operation::PlanB1 { case, selections } => Ok(serde_json::to_value(ExecutionPlan::b1(
            *case,
            selections,
            profile.policy.clone(),
        )?)?),
        Operation::Submit {
            plan,
            approved_digest,
            idempotency_key,
        } => {
            if plan.id()? != approved_digest {
                return Err(invalid("immutable plan approval digest mismatch"));
            }
            check_plan(&plan, profile)?;
            Ok(serde_json::to_value(
                store.submit(&plan, &idempotency_key)?,
            )?)
        }
        Operation::Status { job_id } => Ok(serde_json::to_value(store.job(&job_id)?)?),
        Operation::Logs {
            job_id,
            after,
            limit,
        } => store.logs(&job_id, after, limit),
        Operation::Cancel { job_id } => {
            let job = store.job(&job_id)?;
            if job.state == "queued" {
                store.transition(&job_id, "queued", "cancelled", None)?;
            } else if ["running", "starting", "cancelling"].contains(&job.state.as_str()) {
                if profile.service_mode != "systemd" {
                    return Err(Error::Unqualified(
                        "complete-tree cancellation requires owned systemd service".into(),
                    ));
                }
                let info = unit_info(&job.unit)?;
                if job.invocation_id.is_none()
                    || info.get("InvocationID") != job.invocation_id.as_ref()
                {
                    return Err(invalid(
                        "cancellation requires tracked live invocation identity",
                    ));
                }
                store.transition(&job_id, &job.state, "cancelling", None)?;
                let result = systemctl(&["stop", &job.unit])?;
                if !result.status.success() {
                    return Err(Error::Resource("owned unit cancellation failed".into()));
                }
                store.finish(&job_id, 143, Some("owned complete service tree cancelled"))?;
            }
            Ok(serde_json::to_value(store.job(&job_id)?)?)
        }
        Operation::Artifacts { job_id } => Ok(serde_json::to_value(store.artifacts(&job_id)?)?),
        Operation::Describe { job_id } => {
            let plan = store.plan(&job_id)?;
            Ok(
                serde_json::json!({"job":store.job(&job_id)?,"science_id":plan.case.science_id()?,
                "execution_id":plan.id()?,"presentation_id":digest(&plan.case.presentation)?,
                "artifacts":store.artifacts(&job_id)?,"arrays":"retained in artifacts; not embedded in responses"}),
            )
        }
    }
}
pub fn serve(state: &Path, socket: &Path, profile_path: &Path) -> Result<()> {
    let store = Store::open(state)?;
    let profile = load_profile(profile_path)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(safe_path(state, "worker.lock")?)?;
    lock.try_lock_exclusive()
        .map_err(|_| Error::Resource("one worker per state root".into()))?;
    if socket.parent() != Some(state) {
        return Err(invalid("socket must be inside private state root"));
    }
    if socket.exists() {
        use std::os::unix::fs::FileTypeExt;
        if !fs::symlink_metadata(socket)?.file_type().is_socket() {
            return Err(invalid("refusing to replace non-socket"));
        }
        fs::remove_file(socket)?;
    }
    reconcile(&store, &profile, profile_path, true)?;
    let listener = UnixListener::bind(socket)?;
    fs::set_permissions(socket, fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_read_timeout(Some(Duration::from_secs(2)))?;
                stream.set_write_timeout(Some(Duration::from_secs(2)))?;
                let mut bytes = Vec::new();
                let result = BufReader::new(&stream)
                    .take(MAX_MESSAGE + 1)
                    .read_until(b'\n', &mut bytes);
                let response = match result {
                    Ok(_) if bytes.len() as u64 <= MAX_MESSAGE && bytes.last() == Some(&b'\n') => {
                        match serde_json::from_slice::<WorkerRequest>(&bytes) {
                            Ok(request) => {
                                let result = if request.protocol_version != PROTOCOL_VERSION
                                    || !token(&request.request_id)
                                {
                                    Err(invalid("protocol version/request ID"))
                                } else {
                                    dispatch(&store, &profile, request.request)
                                };
                                reply(request.request_id, result)
                            }
                            Err(e) => reply("invalid".into(), Err(e.into())),
                        }
                    }
                    _ => reply(
                        "invalid".into(),
                        Err(invalid("bounded newline-delimited request required")),
                    ),
                };
                let data = serde_json::to_vec(&response)?;
                if data.len() as u64 > MAX_MESSAGE {
                    let _ = stream
                        .write_all(b"{\"ok\":false,\"error\":{\"code\":\"response_limit\"}}\n");
                } else {
                    let _ = stream.write_all(&data);
                    let _ = stream.write_all(b"\n");
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(25))
            }
            Err(e) => return Err(e.into()),
        }
        reconcile(&store, &profile, profile_path, false)?;
    }
}
pub fn request(socket: &Path, op: Operation) -> Result<Response> {
    let mut stream = UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(Duration::from_secs(15)))?;
    let request = WorkerRequest {
        protocol_version: 1,
        request_id: uuid::Uuid::new_v4().to_string(),
        request: op,
    };
    let mut bytes = serde_json::to_vec(&request)?;
    if bytes.len() as u64 > MAX_MESSAGE {
        return Err(invalid("request limit"));
    }
    bytes.push(b'\n');
    stream.write_all(&bytes)?;
    let mut response = Vec::new();
    BufReader::new(stream)
        .take(MAX_MESSAGE + 1)
        .read_until(b'\n', &mut response)?;
    if response.len() as u64 > MAX_MESSAGE {
        return Err(invalid("response limit"));
    }
    let response: Response = serde_json::from_slice(&response)?;
    if response.request_id != request.request_id || response.protocol_version != 1 {
        return Err(invalid("response identity/version mismatch"));
    }
    Ok(response)
}
pub fn backends() -> serde_json::Value {
    serde_json::json!([
        {"adapter":"channel_reference","backend":"cpu","runtime":"implemented","numerical_verification":"analytical_residual","physical_validation":"unqualified","synthetic":true},
        {"adapter":"freecad","backend":"cpu","runtime":"unqualified","minimum_security_version":"1.1.4"},
        {"adapter":"openlb","backend":"cpu","runtime":"unqualified","precision":"float64","formulation":"periodic_forced_channel","reference_evidence":"docs/qualification.md"},
        {"adapter":"openlb","backend":"cuda","runtime":"unqualified","precision":"float64","formulation":"incompressible BGK D3Q19"},
        {"adapter":"openlb","backend":"hip","runtime":"unqualified","reason":"separate compiler/model/hardware qualification required"},
        {"adapter":"paraview","backend":"egl","runtime":"unqualified"},
        {"adapter":"ffmpeg","backend":"vaapi","runtime":"unqualified"}
    ])
}
pub fn doctor() -> Result<serde_json::Value> {
    Ok(
        serde_json::json!({"schema_version":1,"devices":devices::inventory()?,"fleetix_revision":FLEETIX_REV,
        "fleetix_contract_digest":fleetix_digest(),"backends":backends(),
        "hardware_qualification":"unqualified: inventory is not execution evidence",
        "driver_boundary":"read-only; no driver installation or host activation",
        "service_guarantees":{"systemd":"tracked cgroup and invocation; no automatic logout/boot resume", "foreground":"CI-only; worker restart interrupts jobs"}}),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeRuntime {
    bwrap: String,
    cad: String,
    openlb: Option<String>,
    openlb_backend: String,
    render: String,
    video: String,
}
impl NativeRuntime {
    fn load(path: &Path) -> Result<Self> {
        if !path.starts_with("/nix/store") {
            return Err(invalid(
                "native manifest must be an immutable Nix store path",
            ));
        }
        let runtime: Self = serde_json::from_slice(&read_bounded(path, MAX_MESSAGE)?)?;
        Ok(runtime)
    }
    fn executable(&self, operation: &StageOperation) -> Result<&str> {
        let path = match operation {
            StageOperation::CadFixture | StageOperation::CadInspect => self.cad.as_str(),
            StageOperation::Openlb => self
                .openlb
                .as_deref()
                .ok_or_else(|| Error::Unqualified("OpenLB package absent".into()))?,
            StageOperation::Render => &self.render,
            StageOperation::Video => &self.video,
            _ => return Err(invalid("not a native adapter operation")),
        };
        if !Path::new(path).starts_with("/nix/store") || !Path::new(path).is_file() {
            return Err(Error::Unqualified(
                "exact packaged adapter executable absent".into(),
            ));
        }
        Ok(path)
    }
}
fn disk_bytes(root: &Path) -> Result<u64> {
    let mut total = 0u64;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let m = fs::symlink_metadata(entry.path())?;
        if m.file_type().is_symlink() {
            return Err(invalid("native output symlink rejected"));
        }
        total = total
            .checked_add(if m.is_dir() {
                disk_bytes(&entry.path())?
            } else {
                m.len()
            })
            .ok_or_else(|| invalid("disk accounting overflow"))?;
    }
    Ok(total)
}
fn native_stage(
    store: &Store,
    profile: &HostExecutionProfile,
    plan: &ExecutionPlan,
    stage: &Stage,
    dir: &Path,
    id: &str,
) -> Result<()> {
    let runtime = NativeRuntime::load(Path::new(
        profile
            .native_runtime
            .as_deref()
            .ok_or_else(|| invalid("native runtime"))?,
    ))?;
    let exe = runtime.executable(&stage.operation)?;
    if !Path::new(&runtime.bwrap).starts_with("/nix/store") {
        return Err(invalid("packaged sandbox required"));
    }
    let mut command = Command::new(&runtime.bwrap);
    command
        .env_clear()
        .args([
            "--unshare-all",
            "--die-with-parent",
            "--new-session",
            "--cap-drop",
            "ALL",
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
            "/home",
            "--dir",
            "/home/worker",
            "--setenv",
            "HOME",
            "/home/worker",
            "--setenv",
            "LANG",
            "C.UTF-8",
            "--setenv",
            "PATH",
            "/nonexistent",
            "--setenv",
            "OMP_NUM_THREADS",
        ])
        .arg(profile.threads.to_string())
        .args(["--bind"])
        .arg(dir)
        .arg("/work")
        .args(["--chdir", "/work"]);
    if let Some(selection) = &stage.selection {
        let node = format!("/dev/dri/by-path/pci-{}-render", selection.pci);
        let live = fs::canonicalize(&node)?;
        if selection.role != Role::Compute {
            command.args(["--dev-bind"]).arg(&live).arg(&live);
            command
                .args(["--dir", "/dev/dri/by-path", "--symlink"])
                .arg(&live)
                .arg(&node);
            command.args(["--ro-bind", "/run/opengl-driver", "/run/opengl-driver"]);
        } else {
            // NVIDIA nodes require UUID→minor correlation and a tested per-device mount policy.
            return Err(Error::Unqualified(
                "CUDA per-device sandbox mount policy not hardware-qualified".into(),
            ));
        }
    }
    if matches!(stage.operation, StageOperation::CadInspect) {
        let source = safe_path(
            Path::new(&profile.allowed_input_root),
            &plan.case.geometry.source,
        )?;
        let original = read_bounded(&source, profile.max_disk_bytes)?;
        use sha2::Digest;
        let sha = format!("{:x}", sha2::Sha256::digest(&original));
        if plan.case.geometry.sha256.as_deref() != Some(&sha) {
            return Err(invalid("source CAD checksum mismatch"));
        }
        command.args(["--ro-bind"]).arg(source).arg("/input.FCStd");
    }
    let op = serde_json::to_value(&stage.operation)?
        .as_str()
        .ok_or_else(|| invalid("operation"))?
        .to_owned();
    command
        .args(["--ro-bind"])
        .arg(dir.join("native-plan.json"))
        .arg("/work/plan.json");
    command.arg("--").arg(exe).arg(&op).arg("/work/plan.json");
    let log = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(dir.join(format!("{}.log", stage.id)))?;
    command
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .process_group(0);
    let maximum = profile.max_disk_bytes;
    unsafe {
        command.pre_exec(move || {
            let limit = libc::rlimit {
                rlim_cur: maximum,
                rlim_max: maximum,
            };
            if libc::setrlimit(libc::RLIMIT_FSIZE, &limit) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command.spawn()?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            if !status.success() {
                return Err(Error::Unqualified(format!("native {op} failed: {status}")));
            }
            break;
        }
        let failure = if disk_bytes(dir)? > plan.observation.max_artifact_bytes {
            Some("scientific output budget exhausted")
        } else if start.elapsed().as_secs() > u64::from(profile.timeout_seconds) {
            Some("native time budget exhausted")
        } else {
            None
        };
        if let Some(reason) = failure {
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGTERM);
            }
            std::thread::sleep(Duration::from_millis(200));
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.wait();
            return Err(Error::Resource(reason.into()));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let evidence: serde_json::Value = serde_json::from_slice(&read_bounded(
        &dir.join(format!("{op}-receipt.json")),
        MAX_MESSAGE,
    )?)?;
    if stage.gpu == GpuRequirement::Required
        && (evidence["executed"] != true
            || evidence["software_fallback"] != false
            || evidence["pci"].as_str() != stage.selection.as_ref().map(|s| s.pci.as_str()))
    {
        return Err(Error::Unqualified(
            "missing actual device execution receipt / software fallback rejected".into(),
        ));
    }
    store.event(
        id,
        "native_receipt",
        &serde_json::json!({
            "stage":stage.id, "operation":op, "receipt_sha256":digest(&evidence)?,
            "backend":evidence["backend"], "executed":evidence["executed"],
            "physical_validation":"unqualified"
        })
        .to_string(),
    )?;
    Ok(())
}
pub fn run_job(state: &Path, profile_path: &Path, id: &str) -> Result<()> {
    let store = Store::open(state)?;
    let profile = load_profile(profile_path)?;
    let job = store.job(id)?;
    if !store.transition(id, "starting", "running", None)? {
        return Err(invalid("job must be starting exactly once"));
    }
    let invocation = std::env::var("INVOCATION_ID").ok();
    if profile.service_mode == "systemd" && invocation.as_ref().is_none_or(|v| v.len() != 32) {
        store.finish(id, 1, Some("missing systemd invocation identity"))?;
        return Err(invalid("systemd identity"));
    }
    store.connection.execute(
        "UPDATE jobs SET invocation=?1 WHERE id=?2",
        rusqlite::params![invocation, id],
    )?;
    let result = execute_job(&store, &profile, id);
    store.finish(
        id,
        if result.is_ok() { 0 } else { 1 },
        result.as_ref().err().map(ToString::to_string).as_deref(),
    )?;
    let _ = job;
    result
}
fn execute_job(store: &Store, profile: &HostExecutionProfile, id: &str) -> Result<()> {
    let plan = store.plan(id)?;
    check_plan(&plan, profile)?;
    let dir = store.job_dir(id)?;
    // Hold one physical-card reservation across roles and stages, independent of worker lifetime.
    let reservations = store.root.join("reservations");
    fs::create_dir_all(&reservations)?;
    let mut locks = Vec::new();
    let keys: std::collections::BTreeSet<_> = plan
        .stages
        .iter()
        .filter_map(|s| s.selection.as_ref().map(|g| g.pci.clone()))
        .collect();
    for pci in keys {
        if !pci
            .bytes()
            .all(|c| c.is_ascii_hexdigit() || b":.".contains(&c))
        {
            return Err(invalid("PCI reservation identity"));
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(reservations.join(pci))?;
        let start = Instant::now();
        while file.try_lock_exclusive().is_err() {
            if start.elapsed().as_secs() > u64::from(profile.timeout_seconds) {
                return Err(Error::Resource(
                    "physical card reservation wait expired".into(),
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        locks.push(file);
    }
    let manifest = commit_artifact(
        &dir,
        "plan.json",
        &serde_json::to_vec_pretty(&plan)?,
        "json",
        "immutable approved execution plan",
    )?;
    store.add_artifact(id, &manifest)?;
    let native_work = dir.join(".native-incomplete");
    let mut native_pending = false;
    for stage in &plan.stages {
        store.event(id, "stage_started", &stage.id)?;
        match stage.operation {
            StageOperation::ChannelReference => {
                let c = &plan.case;
                let result = science::channel_reference(
                    c.acceleration.si("acceleration")?,
                    c.channel_height.si("length")?,
                    c.kinematic_viscosity.si("kinematic_viscosity")?,
                    c.resolution as usize,
                )?;
                if result.numerical_error > c.applicability.numerical_tolerance {
                    return Err(invalid(
                        "analytical reference residual outside declared tolerance",
                    ));
                }
                let mut csv = "y_m,velocity_m_s\n".to_string();
                for (y, u) in result.y_m.iter().zip(&result.velocity_m_s) {
                    csv.push_str(&format!("{y:.17e},{u:.17e}\n"));
                }
                let mut artifact = commit_artifact(
                    &dir,
                    "channel.csv",
                    csv.as_bytes(),
                    "csv",
                    "synthetic analytical reference; not OpenLB",
                )?;
                artifact.units = Some("m,m/s".into());
                artifact.time_s = Some(0.);
                artifact.association = Some("point".into());
                store.add_artifact(id, &artifact)?;
                let report = serde_json::json!({"process":"succeeded","convergence":"analytical","numerical_error":result.numerical_error,
                    "mean_velocity_m_s":result.mean_velocity_m_s,"physical_validation":"unqualified",
                    "moisture_risk":"missing humidity, surface temperature and operating history; screening only",
                    "limitations":["synthetic fixture","not a numerical solver run","no cooling, ingress, damage or lifetime claim"],
                    "science_id":c.science_id()?,"execution_id":plan.id()?,"presentation_id":digest(&c.presentation)?});
                store.add_artifact(
                    id,
                    &commit_artifact(
                        &dir,
                        "validation.json",
                        &serde_json::to_vec_pretty(&report)?,
                        "json",
                        "independent analytical reference",
                    )?,
                )?;
            }
            StageOperation::Bundle => {
                if native_pending {
                    for artifact in
                        ingest_native_tree(&native_work, &dir, plan.observation.max_artifact_bytes)?
                    {
                        store.add_artifact(id, &artifact)?;
                    }
                    fs::remove_dir_all(&native_work)?;
                    native_pending = false;
                }
                let bundle = serde_json::json!({"schema_version":1,"relative_paths":true,"plan":plan,
                    "artifacts":store.artifacts(id)?,"fleetix_revision":FLEETIX_REV,"fleetix_contract_digest":fleetix_digest(),
                    "physical_validation":"unqualified","private_geometry_omitted":false});
                store.add_artifact(
                    id,
                    &commit_artifact(
                        &dir,
                        "bundle.json",
                        &serde_json::to_vec_pretty(&bundle)?,
                        "json",
                        "offline bundle index; copy entire job directory",
                    )?,
                )?;
            }
            _ => {
                if !native_pending {
                    private_dir(&native_work)?;
                    commit_artifact(
                        &native_work,
                        "plan.json",
                        &serde_json::to_vec_pretty(&plan)?,
                        "json",
                        "approved native input copy",
                    )?;
                    let mut normalized = plan.clone();
                    let c = &mut normalized.case;
                    for (quantity, dimension, unit) in [
                        (&mut c.length, "length", "m"),
                        (&mut c.channel_height, "length", "m"),
                        (&mut c.geometry_tolerance, "length", "m"),
                        (&mut c.kinematic_viscosity, "kinematic_viscosity", "m2/s"),
                        (&mut c.acceleration, "acceleration", "m/s2"),
                        (&mut c.material.density, "density", "kg/m3"),
                    ] {
                        quantity.value = quantity.si(dimension)?;
                        quantity.unit = unit.into();
                    }
                    commit_artifact(
                        &native_work,
                        "native-plan.json",
                        &serde_json::to_vec_pretty(&normalized)?,
                        "json",
                        "SI adapter view; original quantities remain in immutable plan.json",
                    )?;
                    native_pending = true;
                }
                native_stage(store, profile, &plan, stage, &native_work, id)?;
            }
        }
        if disk_bytes(&dir)? > plan.observation.max_artifact_bytes {
            return Err(Error::Resource(
                "artifact budget exhausted; scientific data retained, job failed".into(),
            ));
        }
        store.event(id, "stage_finished", &stage.id)?;
    }
    if native_pending {
        for artifact in ingest_native_tree(&native_work, &dir, plan.observation.max_artifact_bytes)?
        {
            store.add_artifact(id, &artifact)?;
        }
        fs::remove_dir_all(&native_work)?;
    }
    Ok(())
}
