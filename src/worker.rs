use crate::{
    Error, Result,
    contracts::*,
    devices,
    execution::{ExecutionBinding, packaged_file},
    lifecycle::NativeProcess,
    resources, science,
    storage::*,
};
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
        "--property=LoadState,ActiveState,InvocationID,ExecMainStatus,Result,MainPID,ControlGroup",
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
    if plan.peak_ram() > profile.max_ram_bytes || plan.disk_reservation()? > profile.max_disk_bytes
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
            if matches!(
                stage.operation,
                StageOperation::CadInspect | StageOperation::CadFixture
            ) {
                crate::sandbox::importer_mounts(
                    native.importer_closure()?,
                    Path::new(&native.cad),
                )?;
            }
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
fn launch(store: &Store, capacity: &HostExecutionProfile, id: &str) -> Result<()> {
    let plan = store.plan(id)?;
    let profile = store.job_profile(id)?;
    check_plan(&plan, &profile)?;
    let artifacts = safe_path(&store.root, "artifacts")?;
    let retained_disk = if artifacts.exists() {
        disk_bytes(&artifacts, false)?
    } else {
        0
    };
    if !store.try_start(id, capacity, retained_disk)? {
        return Ok(());
    }
    let binding = store.execution_binding(id)?;
    binding.verify(&plan, &profile)?;
    crate::retention::verify_ready(&store.root, id, &binding, profile.service_mode == "systemd")?;
    let exe = &binding.runner.path;
    let root = fs::canonicalize(&store.root)?;
    let profiles = safe_path(&root, "profiles")?;
    private_dir(&profiles)?;
    let name = format!("{id}.json");
    commit_artifact(
        &profiles,
        &name,
        &serde_json::to_vec(&profile)?,
        "json",
        "immutable effective job profile",
    )?;
    let profile_path = profiles.join(name);
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
        let mut launcher =
            Command::new(option_env!("HARBOR_CAD_SYSTEMD_RUN").unwrap_or("systemd-run"));
        launcher
            .args(["--user", "--quiet", "--service-type=exec", "--unit"])
            .arg(&job.unit)
            .arg("--property=KillMode=control-group")
            .arg("--property=TimeoutStopSec=10s")
            .arg("--property=SendSIGKILL=yes")
            .arg("--property=Restart=no")
            .arg(format!("--property=MemoryMax={}", plan.peak_ram()))
            .arg("--property=TasksMax=128")
            .arg(format!(
                "--property=RuntimeMaxSec={}",
                profile.timeout_seconds
            ))
            .arg("--property=NoNewPrivileges=yes")
            .arg("--property=UMask=0077")
            .arg(format!("--property=CPUQuota={}00%", profile.threads));
        if std::env::var_os("HARBOR_CAD_IMPORT_PROBE_ROOT").is_some() {
            for name in [
                "HARBOR_CAD_IMPORT_PROBE_ROOT",
                "HARBOR_CAD_IMPORT_PROBE_PORT",
                "HARBOR_CAD_CREDENTIAL_SENTINEL",
            ] {
                if let Some(value) = std::env::var_os(name) {
                    let mut argument = std::ffi::OsString::from(format!("--setenv={name}="));
                    argument.push(value);
                    launcher.arg(argument);
                }
            }
        }
        let output = launcher
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
fn reconcile(store: &Store, capacity: &HostExecutionProfile, startup: bool) -> Result<()> {
    for id in store.active()? {
        let job = store.job(&id)?;
        if job.state == "queued" {
            if let Err(e) = launch(store, capacity, &id) {
                store.transition(&id, "queued", "failed", Some(&e.to_string()))?;
                store.transition(&id, "starting", "failed", Some(&e.to_string()))?;
                store.event(&id, "launch_rejected", &e.to_string())?;
            }
        } else if store.job_profile(&id)?.service_mode == "foreground" {
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
            } else if owned_service_closed(store, &job, &info)? {
                let code = info
                    .get("ExecMainStatus")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(1);
                let execution = if job.state == "cancelling" {
                    "cancelled"
                } else {
                    "failed"
                };
                let reason = closed_service_reason(
                    store,
                    &id,
                    execution,
                    "job ended without durable successful exit receipt",
                )?;
                store.finish(&id, if code == 0 { 1 } else { code }, Some(&reason))?;
            }
        }
    }
    store.cleanup_retention(|job| {
        if store.job_profile(&job.id)?.service_mode == "foreground" {
            return Ok(true);
        }
        owned_service_closed(store, job, &unit_info(&job.unit)?)
    })?;
    Ok(())
}
fn owned_service_closed(store: &Store, job: &Job, info: &BTreeMap<String, String>) -> Result<bool> {
    let missing = info.get("LoadState").map(String::as_str) == Some("not-found");
    if !missing
        && (!info
            .get("ActiveState")
            .is_some_and(|s| s == "inactive" || s == "failed")
            || job.invocation_id.as_ref().is_some_and(|id| {
                info.get("InvocationID")
                    .is_some_and(|observed| !observed.is_empty() && observed != id)
            }))
    {
        return Ok(false);
    }
    let mut groups: Vec<String> = info
        .get("ControlGroup")
        .filter(|g| !g.is_empty())
        .cloned()
        .into_iter()
        .collect();
    let owner = safe_path(
        &store.root,
        &format!("artifacts/{}/service-owner.json", job.id),
    )?;
    if owner.exists() {
        let owner = read_native_receipt(&owner)?;
        if owner["unit"] != job.unit
            || job
                .invocation_id
                .as_ref()
                .is_some_and(|id| owner["invocation"] != *id)
        {
            return Err(invalid("persisted service owner differs from job identity"));
        }
        groups.push(
            owner["control_group"]
                .as_str()
                .ok_or_else(|| invalid("recorded service cgroup missing"))?
                .into(),
        );
    }
    for group in groups {
        let relative = group
            .strip_prefix('/')
            .ok_or_else(|| invalid("absolute service cgroup required"))?;
        let path = safe_path(Path::new("/sys/fs/cgroup"), relative)?;
        match fs::read_to_string(path.join("cgroup.events")) {
            Ok(events) if events.lines().any(|l| l == "populated 0") => {}
            Ok(_) => return Ok(false),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(true)
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
        Operation::PlanCadInspection {
            case,
            max_artifact_bytes,
        } => Ok(serde_json::to_value(ExecutionPlan::cad_inspection(
            *case,
            profile.policy.clone(),
            max_artifact_bytes,
        )?)?),
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
            if let Some(job) = store.existing_submission(&plan, &idempotency_key, profile)? {
                return Ok(serde_json::to_value(job)?);
            }
            check_plan(&plan, profile)?;
            let mut files = BTreeMap::new();
            if let Some(path) = &profile.native_runtime {
                let runtime = NativeRuntime::load(Path::new(path))?;
                files.insert("bwrap".into(), runtime.bwrap.clone());
                for stage in &plan.stages {
                    if !matches!(
                        stage.operation,
                        StageOperation::Bundle | StageOperation::ChannelReference
                    ) {
                        files.insert(
                            serde_json::to_string(&stage.operation)?,
                            runtime.executable(&stage.operation)?.into(),
                        );
                        if matches!(
                            stage.operation,
                            StageOperation::CadInspect | StageOperation::CadFixture
                        ) {
                            files.insert(
                                "cad_closure".into(),
                                runtime
                                    .importer_closure()?
                                    .to_str()
                                    .ok_or_else(|| invalid("importer closure path"))?
                                    .into(),
                            );
                        }
                    }
                }
            }
            let binding =
                ExecutionBinding::capture(&plan, profile, &std::env::current_exe()?, files)?;
            Ok(serde_json::to_value(store.submit_for_execution(
                &plan,
                &idempotency_key,
                profile,
                &binding,
            )?)?)
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
                if store.job_profile(&job_id)?.service_mode != "systemd" {
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
                if !owned_service_closed(store, &store.job(&job_id)?, &unit_info(&job.unit)?)? {
                    return Err(Error::Resource(
                        "owned service tree has not fully terminated; retention preserved".into(),
                    ));
                }
                let reason = closed_service_reason(
                    store,
                    &job_id,
                    "cancelled",
                    "owned complete service tree cancelled",
                )?;
                store.finish(&job_id, 143, Some(&reason))?;
            }
            Ok(serde_json::to_value(store.job(&job_id)?)?)
        }
        Operation::Artifacts {
            job_id,
            after,
            limit,
        } => Ok(serde_json::to_value(store.artifact_page(
            &job_id,
            after.as_deref(),
            limit,
        )?)?),
        Operation::Describe { job_id } => {
            let plan = store.plan(&job_id)?;
            let binding = match store.execution_binding(&job_id) {
                Ok(binding) => Some(binding),
                Err(Error::Unqualified(_)) => None,
                Err(error) => return Err(error),
            };
            Ok(
                serde_json::json!({"job":store.job(&job_id)?,"science_id":plan.case.science_id()?,
                "execution_id":plan.id()?,"presentation_id":digest(&plan.case.presentation)?,
                "execution_binding":binding,
                "artifacts":store.artifact_page(&job_id, None, default_artifact_limit())?,"arrays":"retained in artifacts; not embedded in responses"}),
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
    reconcile(&store, &profile, true)?;
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
                let data = if data.len() as u64 >= MAX_MESSAGE {
                    serde_json::to_vec(&reply(
                        response.request_id,
                        Err(Error::Resource("response limit; use bounded pages".into())),
                    ))?
                } else {
                    data
                };
                let _ = stream.write_all(&data);
                let _ = stream.write_all(b"\n");
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(25))
            }
            Err(e) => return Err(e.into()),
        }
        reconcile(&store, &profile, false)?;
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
        {"adapter":"openlb","backend":"hip","runtime":"unqualified","priority":"primary","precision":"float64","formulation":"periodic_forced_channel","reason":"compiler/model/hardware and KFD sandbox qualification required","decision_evidence":"docs/gpu-backends.md"},
        {"adapter":"openlb","backend":"cuda","runtime":"unqualified","priority":"best_effort","precision":"float64","formulation":"periodic_forced_channel"},
        {"adapter":"openlb","backend":"vulkan","runtime":"unsupported","reason":"no Vulkan backend in pinned OpenLB; Float64 and workload performance require separate evidence","decision_evidence":"docs/gpu-backends.md"},
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
    #[serde(default)]
    cad_closure: Option<String>,
    openlb: Option<String>,
    openlb_backend: String,
    render: Option<String>,
    video: Option<String>,
}
impl NativeRuntime {
    fn importer_closure(&self) -> Result<&Path> {
        self.cad_closure.as_deref().map(Path::new).ok_or_else(|| {
            Error::Unqualified("runtime lacks operation-specific importer closure policy".into())
        })
    }
    fn load(path: &Path) -> Result<Self> {
        let path = packaged_file(path)?;
        let runtime: Self = serde_json::from_slice(&read_bounded(&path, MAX_MESSAGE)?)?;
        packaged_file(Path::new(&runtime.bwrap))?;
        Ok(runtime)
    }
    fn executable(&self, operation: &StageOperation) -> Result<&str> {
        let path = match operation {
            StageOperation::CadFixture | StageOperation::CadInspect => self.cad.as_str(),
            StageOperation::Openlb => self
                .openlb
                .as_deref()
                .ok_or_else(|| Error::Unqualified("OpenLB package absent".into()))?,
            StageOperation::Render => self.render.as_deref().ok_or_else(|| {
                Error::Unqualified("EGL renderer absent from selected runtime".into())
            })?,
            StageOperation::Video => self.video.as_deref().ok_or_else(|| {
                Error::Unqualified("hardware media adapter absent from selected runtime".into())
            })?,
            _ => return Err(invalid("not a native adapter operation")),
        };
        packaged_file(Path::new(path))?;
        Ok(path)
    }
}
fn disk_bytes(root: &Path, reject_unsafe: bool) -> Result<u64> {
    fn visit(root: &Path, reject_unsafe: bool, depth: u32, entries: &mut u64) -> Result<u64> {
        if depth > 64 {
            return Err(Error::Resource(
                "bounded disk inventory depth exhausted".into(),
            ));
        }
        let mut total = 0u64;
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            *entries += 1;
            if *entries > 131072 {
                return Err(Error::Resource(
                    "bounded disk inventory entries exhausted".into(),
                ));
            }
            let m = fs::symlink_metadata(entry.path())?;
            if reject_unsafe && !m.is_dir() && !m.is_file() {
                return Err(invalid("native output symlink/special entry rejected"));
            }
            total = total
                .checked_add(if m.is_dir() {
                    visit(&entry.path(), reject_unsafe, depth + 1, entries)?
                } else {
                    m.len()
                })
                .ok_or_else(|| invalid("disk accounting overflow"))?;
        }
        Ok(total)
    }
    // Quarantined failures count against admission, but never follow their
    // symlinks or let an unsafe raw entry poison all later jobs in the root.
    visit(root, reject_unsafe, 0, &mut 0)
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
    let mut command = Command::new(&runtime.bwrap);
    command
        .env_clear()
        .args([
            "--unshare-all",
            "--die-with-parent",
            "--new-session",
            "--cap-drop",
            "ALL",
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
    if matches!(
        stage.operation,
        StageOperation::CadInspect | StageOperation::CadFixture
    ) {
        crate::sandbox::mount_importer(&mut command, runtime.importer_closure()?, Path::new(exe))?;
    } else {
        command.args(["--ro-bind", "/nix/store", "/nix/store"]);
    }
    if let Some(selection) = &stage.selection {
        if selection.role != Role::Compute {
            let binding = devices::DrmSandbox::resolve(&selection.pci)?;
            binding.apply(&mut command);
            store.event(
                id,
                "dri_binding_verified",
                &serde_json::to_string(&binding)?,
            )?;
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
        let job_dir = store.job_dir(id)?;
        let artifact = snapshot_cad_input(
            &source,
            &job_dir,
            plan.case
                .geometry
                .sha256
                .as_deref()
                .ok_or_else(|| invalid("source CAD digest required"))?,
            plan.observation.max_artifact_bytes,
        )?;
        store.add_artifact(id, &artifact)?;
        command
            .args(["--ro-bind"])
            .arg(safe_path(&job_dir, &artifact.path)?)
            .arg("/input.FCStd");
    }
    let op = serde_json::to_value(&stage.operation)?
        .as_str()
        .ok_or_else(|| invalid("operation"))?
        .to_owned();
    let receipt = safe_path(dir, &format!("{op}-receipt.json"))?;
    if fs::symlink_metadata(&receipt).is_ok() {
        return Err(invalid(
            "native receipt already exists before its stage; stale or forged output rejected",
        ));
    }
    command
        .args(["--ro-bind"])
        .arg(safe_path(&store.job_dir(id)?, "native-plan.json")?)
        .arg("/plan.json");
    command.arg("--").arg(exe).arg(&op).arg("/plan.json");
    let log = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(dir.join(format!("{}.log", stage.id)))?;
    command
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
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
    let mut child = NativeProcess::spawn(command)?;
    let status = child.wait(
        Duration::from_secs(u64::from(profile.timeout_seconds)),
        || {
            if disk_bytes(dir, true)? > plan.observation.max_artifact_bytes {
                return Err(Error::Resource("scientific output budget exhausted".into()));
            }
            Ok(())
        },
    )?;
    if !status.success() {
        return Err(Error::Unqualified(format!("native {op} failed: {status}")));
    }
    let evidence = read_native_receipt(&receipt)?;
    validate_native_receipt(stage, &evidence)?;
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

fn read_native_receipt(path: &Path) -> Result<serde_json::Value> {
    let input = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let metadata = input.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_MESSAGE {
        return Err(invalid("native receipt must be a bounded regular file"));
    }
    let mut bytes = Vec::new();
    input.take(MAX_MESSAGE + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_MESSAGE {
        return Err(invalid("native receipt grew beyond its response budget"));
    }
    Ok(serde_json::from_slice(&bytes)?)
}

fn validate_native_receipt(stage: &Stage, evidence: &serde_json::Value) -> Result<()> {
    let adapter = match stage.operation {
        StageOperation::CadFixture | StageOperation::CadInspect => "FreeCAD",
        StageOperation::Openlb => "OpenLB",
        StageOperation::Render => "ParaView",
        StageOperation::Video => "FFmpeg",
        _ => return Err(invalid("not a native adapter operation")),
    };
    let backend = stage
        .selection
        .as_ref()
        .map_or("cpu", |s| s.backend.as_str());
    if evidence["adapter"] != adapter
        || evidence["backend"] != backend
        || evidence["executed"] != true
        || evidence["software_fallback"] != false
        || (matches!(
            stage.operation,
            StageOperation::CadFixture | StageOperation::CadInspect
        ) && evidence["import_policy"] != crate::sandbox::IMPORT_POLICY)
        || (stage.gpu == GpuRequirement::Required
            && evidence["pci"].as_str() != stage.selection.as_ref().map(|s| s.pci.as_str()))
    {
        return Err(Error::Unqualified(
            "adapter/backend/execution receipt mismatch; fallback rejected".into(),
        ));
    }
    Ok(())
}

pub fn run_job(state: &Path, profile_path: &Path, id: &str) -> Result<()> {
    let store = Store::open(state)?;
    let profile = load_profile(profile_path)?;
    if digest(&profile)? != digest(&store.job_profile(id)?)? {
        return Err(invalid(
            "effective profile differs from immutable job binding",
        ));
    }
    let job = store.job(id)?;
    let invocation = if profile.service_mode == "systemd" {
        let invocation = std::env::var("INVOCATION_ID")
            .map_err(|_| invalid("missing systemd invocation identity"))?;
        if invocation.len() != 32 || !invocation.bytes().all(|v| v.is_ascii_hexdigit()) {
            return Err(invalid("invalid systemd invocation identity"));
        }
        let info = unit_info(&job.unit)?;
        let group = info
            .get("ControlGroup")
            .filter(|v| !v.is_empty())
            .ok_or_else(|| invalid("missing owned service cgroup"))?;
        let membership = fs::read_to_string("/proc/self/cgroup")?;
        if info.get("InvocationID") != Some(&invocation)
            || info.get("MainPID").and_then(|v| v.parse::<u32>().ok()) != Some(std::process::id())
            || !membership
                .lines()
                .any(|line| line.strip_prefix("0::") == Some(group.as_str()))
            || info.get("LoadState").map(String::as_str) != Some("loaded")
            || !info
                .get("ActiveState")
                .is_some_and(|v| v == "active" || v == "activating")
        {
            return Err(invalid(
                "job execution requires its live systemd invocation, main PID and exact cgroup",
            ));
        }
        let owner = serde_json::json!({"unit":job.unit,"invocation":invocation,"main_pid":std::process::id(),"control_group":group});
        store.add_artifact(
            id,
            &commit_artifact(
                &store.job_dir(id)?,
                "service-owner.json",
                &serde_json::to_vec_pretty(&owner)?,
                "json",
                "live systemd unit, main PID and exact cgroup verified before job execution",
            )?,
        )?;
        store.event(id, "service_owner_verified", &owner.to_string())?;
        Some(invocation)
    } else {
        None
    };
    let binding = store.execution_binding(id)?;
    binding.verify(&store.plan(id)?, &profile)?;
    binding.verify_running_runner(&profile)?;
    crate::retention::verify_ready(&store.root, id, &binding, profile.service_mode == "systemd")?;
    if !store.transition(id, "starting", "running", None)? {
        return Err(invalid("job must be starting exactly once"));
    }
    store.connection.execute(
        "UPDATE jobs SET invocation=?1 WHERE id=?2",
        rusqlite::params![invocation, id],
    )?;
    let result = execute_job(&store, &profile, id);
    if let Err(error) = &result {
        let saved = retain_native_failure(
            &store,
            id,
            "failed",
            &serde_json::to_value(error.diagnostic())?,
        );
        if let Err(retention) = saved {
            store.event(id, "native_failure_retention_error", &retention.to_string())?;
        }
    }
    store.finish(
        id,
        if result.is_ok() { 0 } else { 1 },
        result.as_ref().err().map(ToString::to_string).as_deref(),
    )?;
    let _ = job;
    result
}

/// Called after adapter cleanup or confirmed owned-service termination. A
/// killed job cannot run its own cleanup; the persistent worker recovers the
/// bounded closed records before publishing its terminal state.
fn retain_native_failure(
    store: &Store,
    id: &str,
    execution: &str,
    cause: &serde_json::Value,
) -> Result<()> {
    let dir = store.job_dir(id)?;
    let raw = safe_path(&dir, ".native-incomplete")?;
    if !raw.exists() {
        return Ok(());
    }
    let registered: bool = store.connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM artifacts WHERE job=?1 AND path='native-failure.json')",
        [id],
        |row| row.get(0),
    )?;
    if registered {
        return Ok(());
    }
    let snapshot = retain_failed_native_tree(
        &raw,
        &dir,
        store.recorded_plan(id)?.observation.max_artifact_bytes,
    )?;
    for artifact in &snapshot.artifacts {
        store.add_artifact(id, artifact)?;
    }
    let report = serde_json::json!({"schema_version":1,"execution":execution,"physical_validation":"unqualified",
        "cause":cause,"snapshot":snapshot,"raw_tree_retained":".native-incomplete",
        "partial_files":"opaque failed-attempt bytes; not qualified scientific outputs"});
    let manifest = commit_artifact(
        &dir,
        "native-failure.json",
        &serde_json::to_vec_pretty(&report)?,
        "json",
        "closed failed/cancelled native output inventory; omissions are explicit",
    )?;
    store.add_artifact(id, &manifest)?;
    Ok(())
}

fn closed_service_reason(store: &Store, id: &str, execution: &str, reason: &str) -> Result<String> {
    if let Err(error) =
        retain_native_failure(store, id, execution, &serde_json::json!({"reason":reason}))
    {
        store.event(id, "native_failure_retention_error", &error.to_string())?;
        return Ok(format!(
            "{reason}; raw output quarantined; recovery error: {error}"
        ));
    }
    Ok(reason.into())
}
fn execute_job(store: &Store, profile: &HostExecutionProfile, id: &str) -> Result<()> {
    let plan = store.plan(id)?;
    check_plan(&plan, profile)?;
    let dir = store.job_dir(id)?;
    store.add_artifact(
        id,
        &commit_artifact(
            &dir,
            "execution-binding.json",
            &serde_json::to_vec_pretty(&store.execution_binding(id)?)?,
            "json",
            "exact runner, runtime, host profile and sandbox policy binding",
        )?,
    )?;
    // Hold one physical-card reservation across roles and stages, independent of worker lifetime.
    let keys: Vec<_> = plan
        .stages
        .iter()
        .filter_map(|s| s.selection.as_ref().map(|g| g.pci.clone()))
        .collect();
    let _locks = if keys.is_empty() {
        Vec::new()
    } else {
        let reservations = resources::card_reservation_root()?;
        let start = Instant::now();
        loop {
            if let Some(locks) = resources::try_reserve_cards(&reservations, &keys)? {
                break locks;
            }
            if start.elapsed().as_secs() > u64::from(profile.timeout_seconds) {
                return Err(Error::Resource(
                    "physical card reservation wait expired".into(),
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    };
    let manifest = commit_artifact(
        &dir,
        "plan.json",
        &serde_json::to_vec_pretty(&plan)?,
        "json",
        "immutable approved execution plan",
    )?;
    store.add_artifact(id, &manifest)?;
    store.add_artifact(
        id,
        &commit_artifact(
            &dir,
            "host-profile.json",
            &serde_json::to_vec_pretty(profile)?,
            "json",
            "immutable effective host policy and resource limits",
        )?,
    )?;
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
                    let runtime = packaged_file(Path::new(
                        profile
                            .native_runtime
                            .as_deref()
                            .ok_or_else(|| invalid("native runtime"))?,
                    ))?;
                    store.add_artifact(id, &commit_artifact(&dir, "native-runtime.json", &read_bounded(&runtime, MAX_MESSAGE)?, "json",
                        &format!("exact immutable runtime manifest from {}; store paths bind packaged executable closures", runtime.display()))?)?;
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
                    store.add_artifact(id, &commit_artifact(
                        &dir,
                        "native-plan.json",
                        &serde_json::to_vec_pretty(&normalized)?,
                        "json",
                        "trusted SI adapter view; exposed only through read-only sandbox mount; original quantities in plan.json",
                    )?)?;
                    native_pending = true;
                }
                native_stage(store, profile, &plan, stage, &native_work, id)?;
            }
        }
        if disk_bytes(&dir, true)? > plan.observation.max_artifact_bytes {
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

#[cfg(test)]
mod tests {
    use super::{disk_bytes, packaged_file, read_native_receipt, validate_native_receipt};
    use crate::contracts::{CaseSpec, ExecutionPlan};
    use std::{fs, path::Path};

    #[test]
    fn lexical_store_prefix_cannot_authorize_mutable_native_code() {
        let temp = tempfile::tempdir().unwrap();
        let outside = temp.path().join("manifest.json");
        fs::write(&outside, b"{}").unwrap();
        let escape = Path::new("/nix/store/../..").join(outside.strip_prefix("/").unwrap());
        assert!(matches!(
            packaged_file(&escape),
            Err(crate::Error::Invalid(_))
        ));
        assert!(matches!(
            packaged_file(&outside),
            Err(crate::Error::Invalid(_))
        ));
        assert!(packaged_file(Path::new("/nix/store")).is_err());
    }

    #[test]
    fn quarantined_unsafe_entries_are_counted_without_following_or_poisoning_admission() {
        let temp = tempfile::tempdir().unwrap();
        let raw = temp.path().join("quarantined");
        fs::create_dir(&raw).unwrap();
        let outside = temp.path().join("outside");
        fs::write(&outside, vec![0u8; 1048576]).unwrap();
        fs::write(raw.join("log"), b"fail").unwrap();
        std::os::unix::fs::symlink(&outside, raw.join("escape")).unwrap();
        assert!(disk_bytes(&raw, true).is_err());
        assert_eq!(
            disk_bytes(&raw, false).unwrap(),
            4 + fs::symlink_metadata(raw.join("escape")).unwrap().len()
        );
    }

    #[test]
    fn cpu_native_success_requires_matching_executed_adapter_receipts() {
        let mut case = CaseSpec::reference();
        case.applicability.formulation = "periodic_forced_channel".into();
        case.acceleration.value = 0.001;
        let plan = ExecutionPlan::openlb_reference(case, "research".into()).unwrap();
        let flow = &plan.stages[1];
        let valid = serde_json::json!({"adapter":"OpenLB","backend":"cpu","executed":true,"software_fallback":false});
        validate_native_receipt(flow, &valid).unwrap();
        assert!(validate_native_receipt(flow, &serde_json::json!({})).is_err());
        for (field, value) in [
            ("adapter", serde_json::json!("FreeCAD")),
            ("backend", serde_json::json!("cuda")),
            ("executed", serde_json::json!(false)),
            ("software_fallback", serde_json::json!(true)),
        ] {
            let mut corrupted = valid.clone();
            corrupted[field] = value;
            assert!(validate_native_receipt(flow, &corrupted).is_err());
        }
    }

    #[test]
    fn native_receipt_rejects_special_symlink_and_oversized_records() {
        use std::{
            ffi::CString,
            os::unix::{ffi::OsStrExt, fs::symlink},
        };
        let temp = tempfile::tempdir().unwrap();
        let regular = temp.path().join("receipt.json");
        fs::write(&regular, br#"{"executed":true}"#).unwrap();
        assert_eq!(read_native_receipt(&regular).unwrap()["executed"], true);
        let link = temp.path().join("alias.json");
        symlink(&regular, &link).unwrap();
        assert!(read_native_receipt(&link).is_err());
        let fifo = temp.path().join("pipe.json");
        let name = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        // SAFETY: name is NUL-terminated and lives for the complete call.
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        assert!(read_native_receipt(&fifo).is_err());
        fs::write(&regular, vec![b' '; super::MAX_MESSAGE as usize + 1]).unwrap();
        assert!(read_native_receipt(&regular).is_err());
    }
}
