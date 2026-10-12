//! Executed application captures with durable restore acceptance.
pub use super::application_retention::prune;
use super::{
    Result, application_publication as publication, codec, durable, invalid, process, string,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    os::{
        fd::RawFd,
        unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
pub fn identity(v: &Value) -> Result<String> {
    Ok(codec::digest(&codec::encode(v, true)?))
}
fn exact(v: &Value, keys: &[&str]) -> bool {
    v.as_object()
        .is_some_and(|m| m.len() == keys.len() && keys.iter().all(|k| m.contains_key(*k)))
}
pub fn validate(c: &Value, check_root: bool) -> Result<PathBuf> {
    let protected = c["version"] == 2;
    let mut keys = vec![
        "version",
        "resource",
        "root",
        "commands",
        "executable_files",
        "timeout_seconds",
        "maximum_age_seconds",
    ];
    if protected {
        keys.push("worker_user");
        if c.get("credentials").is_some() {
            keys.push("credentials");
            publication::validate_credentials(&c["credentials"])?;
        }
    }
    if !exact(c, &keys) || (!protected && c["version"] != 1) {
        return Err(invalid("unsupported application backup manifest"));
    }
    if protected && !super::resource::valid_name(string(c, "worker_user")?) {
        return Err(invalid("invalid backup worker account"));
    }
    if !super::resource::valid_name(string(c, "resource")?) {
        return Err(invalid("invalid application resource"));
    }
    let root = PathBuf::from(string(c, "root")?);
    if !root.is_absolute() || (check_root && (!root.is_dir() || fs::canonicalize(&root)? != root)) {
        return Err(invalid("backup root is missing or redirected"));
    }
    for key in ["timeout_seconds", "maximum_age_seconds"] {
        if !c[key].as_u64().is_some_and(|n| n > 0 && n <= 86400) {
            return Err(invalid(
                "application backup limits must be positive and bounded",
            ));
        }
    }
    if !exact(&c["commands"], &["capture", "restore", "verify", "cleanup"]) {
        return Err(invalid(
            "capture, restore and semantic verifier commands are mandatory",
        ));
    }
    for (stage, v) in c["commands"].as_object().unwrap() {
        let argv = argv(v)?;
        if argv.is_empty()
            || !Path::new(&argv[0]).is_absolute()
            || !argv.iter().any(|s| s == "{backup}")
            || (stage != "capture" && !argv.iter().any(|s| s == "{workspace}"))
        {
            return Err(invalid(
                "commands require an absolute executable and explicit artifact/workspace arguments",
            ));
        }
        if protected {
            immutable_file(&argv[0], true)?;
        }
    }
    for path in argv(&c["executable_files"])? {
        if protected {
            immutable_file(&path, false)?;
        }
    }
    Ok(root)
}
fn immutable_file(path: &str, executable: bool) -> Result<PathBuf> {
    super::cutover::validate_path(path)?;
    let resolved = durable::immutable_store_reference(Path::new(path)).map_err(|error| {
        invalid(format!(
            "protected backup requires immutable store files: {error}"
        ))
    })?;
    if executable && fs::metadata(&resolved)?.mode() & 0o111 == 0 {
        return Err(invalid(
            "protected backup command requires an executable store file",
        ));
    }
    Ok(resolved)
}
fn argv(v: &Value) -> Result<Vec<String>> {
    v.as_array()
        .ok_or_else(|| invalid("invalid command arguments"))?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid("invalid command argument"))
        })
        .collect()
}
fn executables(c: &Value) -> Result<Value> {
    let mut paths: BTreeSet<_> = argv(&c["executable_files"])?.into_iter().collect();
    for v in c["commands"]
        .as_object()
        .ok_or_else(|| invalid("invalid commands"))?
        .values()
    {
        paths.insert(
            argv(v)?
                .first()
                .ok_or_else(|| invalid("empty command"))?
                .clone(),
        );
    }
    let mut result = serde_json::Map::new();
    for s in paths {
        let p = Path::new(&s);
        if !p.is_absolute() || !p.is_file() {
            return Err(invalid("backup executable or adapter file is absent"));
        }
        let resolved = if c["version"] == 2 {
            immutable_file(&s, false)?
        } else {
            fs::canonicalize(p)?
        };
        let digest = codec::file_digest(&resolved)?;
        result.insert(s, json!(digest));
    }
    Ok(Value::Object(result))
}
fn walk(root: &Path, path: &Path, result: &mut serde_json::Map<String, Value>) -> Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let entry_path = entry.path();
        let relative = entry_path
            .strip_prefix(root)
            .map_err(|_| invalid("invalid artifact path"))?
            .to_str()
            .ok_or_else(|| invalid("backup artifact path is not UTF-8"))?;
        if kind.is_dir() {
            if result
                .insert(relative.into(), json!({"directory":true}))
                .is_some()
            {
                return Err(invalid("backup artifact path is duplicated"));
            }
            walk(root, &entry_path, result)?;
        } else if kind.is_file() {
            if fs::symlink_metadata(&entry_path)?.nlink() != 1 {
                return Err(invalid("backup artifacts must not have hard links"));
            }
            if result
                .insert(relative.into(), json!(codec::file_digest(&entry_path)?))
                .is_some()
            {
                return Err(invalid("backup artifact path is duplicated"));
            }
        } else {
            return Err(invalid("backup contains redirected or special files"));
        }
    }
    Ok(())
}
pub fn inventory(path: &Path) -> Result<Value> {
    if !fs::symlink_metadata(path)?.is_dir() {
        return Err(invalid("backup directory is missing or redirected"));
    }
    let mut result = serde_json::Map::new();
    walk(path, path, &mut result)?;
    if result.is_empty() {
        return Err(invalid("application capture produced no artifacts"));
    }
    Ok(Value::Object(result))
}
fn execute(
    c: &Value,
    stage: &str,
    backup: &Path,
    workspace: &Path,
    lease: RawFd,
    worker: Option<&publication::Worker>,
) -> Result<Vec<u8>> {
    let command = argv(&c["commands"][stage])?;
    if c["version"] == 2 {
        immutable_file(
            command.first().ok_or_else(|| invalid("empty command"))?,
            true,
        )?;
    }
    let args = command
        .into_iter()
        .map(|s| match s.as_str() {
            "{backup}" => backup.to_string_lossy().into_owned(),
            "{workspace}" => workspace.to_string_lossy().into_owned(),
            _ => s,
        })
        .collect();
    let mut env: BTreeMap<_, _> = std::env::vars()
        .filter(|(k, _)| {
            [
                "PATH",
                "HOME",
                "USER",
                "LOGNAME",
                "LANG",
                "LC_ALL",
                "TMPDIR",
                "CREDENTIALS_DIRECTORY",
            ]
            .contains(&k.as_str())
        })
        .collect();
    let mut leases = BTreeSet::from([lease]);
    for s in std::env::var("HARBOR_DB_LEASE_FDS")
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
    {
        let fd: RawFd = s
            .parse()
            .map_err(|_| invalid("invalid inherited Harbor DB lease"))?;
        if fd < 3 || unsafe { libc::fcntl(fd, libc::F_GETFD) } < 0 {
            return Err(invalid("invalid inherited Harbor DB lease"));
        }
        leases.insert(fd);
    }
    env.insert(
        "HARBOR_DB_LEASE_FDS".into(),
        leases
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(","),
    );
    env.insert(
        "HARBOR_DB_APPLICATION_TIMEOUT_SECONDS".into(),
        c["timeout_seconds"].to_string(),
    );
    let mut spec = process::CommandSpec::new(args);
    if let Some(worker) = worker {
        spec.identity = Some(worker.identity.clone());
        env.insert("USER".into(), string(c, "worker_user")?.into());
        env.insert("LOGNAME".into(), string(c, "worker_user")?.into());
        env.insert("HOME".into(), workspace.to_string_lossy().into_owned());
        if let Some(credentials) = &worker.credentials {
            env.insert(
                "CREDENTIALS_DIRECTORY".into(),
                credentials.to_string_lossy().into_owned(),
            );
        }
    }
    spec.environment = Some(env);
    spec.cwd = Some(workspace.to_owned());
    spec.leases = leases.into_iter().collect();
    spec.timeout = Duration::from_secs(
        c["timeout_seconds"]
            .as_u64()
            .ok_or_else(|| invalid("invalid timeout"))?,
    );
    process::execute(&spec)
}
fn hostname() -> Result<String> {
    let mut buf = [0u8; 256];
    if unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let end = buf
        .iter()
        .position(|b| *b == 0)
        .ok_or_else(|| invalid("hostname too long"))?;
    String::from_utf8(buf[..end].to_vec()).map_err(|_| invalid("hostname is not UTF-8"))
}
pub(super) fn machine() -> Result<String> {
    // Sandboxed Cargo qualification supplies an immutable fixture at compile
    // time. Runtime environment cannot replace machine custody, and lifecycle
    // packages built without `testing` always require the installed identity.
    #[cfg(feature = "testing")]
    let path = option_env!("HARBOR_DB_TEST_MACHINE_ID").unwrap_or("/etc/machine-id");
    #[cfg(not(feature = "testing"))]
    let path = "/etc/machine-id";
    codec::file_digest(&fs::canonicalize(path)?)
}
fn capture_receipt(backup: &Path) -> Result<Value> {
    const LIMIT: u64 = 4096;
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(backup.join("capture.json"))?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(invalid("capture receipt must be a regular file"));
    }
    if metadata.len() > LIMIT {
        return Err(invalid("capture receipt exceeds the 4096-byte limit"));
    }
    let mut bytes = Vec::new();
    (&mut file).take(LIMIT + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > LIMIT {
        return Err(invalid("capture receipt exceeds the 4096-byte limit"));
    }
    codec::decode(&bytes)
}
fn now() -> Result<i64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| invalid("clock precedes epoch"))?
        .as_secs() as i64)
}
fn unique() -> Result<u128> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| invalid("clock precedes epoch"))?
        .as_nanos())
}
fn private_dir(p: &Path) -> Result<()> {
    fs::DirBuilder::new().mode(0o700).create(p)?;
    Ok(())
}
fn certification(
    c: &Value,
    b: &Path,
    w: &Path,
    l: RawFd,
    a: &Value,
    worker: Option<&publication::Worker>,
) -> Result<Value> {
    execute(c, "restore", b, w, l, worker)?;
    let receipt = super::codec::decode(&execute(c, "verify", b, w, l, worker)?)?;
    let captured = capture_receipt(b)?;
    let semantic = captured
        .get("semantic_sha256")
        .and_then(Value::as_str)
        .unwrap_or("");
    if !exact(&captured, &["version", "consistency", "semantic_sha256"])
        || captured["version"] != 1
        || !["quiesced", "shared_exported_mvcc_snapshot"]
            .contains(&captured["consistency"].as_str().unwrap_or(""))
        || semantic.len() != 64
        || !semantic
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid(
            "application capture requires complete semantic identity and a declared consistency window",
        ));
    }
    if !exact(&receipt, &["version", "status", "semantic_sha256"])
        || receipt["version"] != 1
        || receipt["status"] != "verified"
        || receipt["semantic_sha256"] != captured["semantic_sha256"]
    {
        return Err(invalid(
            "application restore verifier did not accept the complete captured semantic identity",
        ));
    }
    if inventory(b)? != *a {
        return Err(invalid(
            "backup artifact hash changed during restore verification",
        ));
    }
    Ok(
        json!({"version":1,"status":"verified","executor":hostname()?,"executor_machine_sha256":machine()?,"semantic_sha256":semantic,"artifacts":a,"consistency":captured["consistency"]}),
    )
}
fn permissions(path: &Path) -> Result<()> {
    for e in fs::read_dir(path)? {
        let e = e?;
        if e.file_type()?.is_dir() {
            permissions(&e.path())?;
        } else if e.file_type()?.is_file() {
            if fs::symlink_metadata(e.path())?.nlink() != 1 {
                return Err(invalid("backup artifacts must not have hard links"));
            }
            fs::set_permissions(e.path(), fs::Permissions::from_mode(0o440))?;
        } else {
            return Err(invalid("backup contains redirected or special files"));
        }
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o750))?;
    Ok(())
}
fn exists(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok()
}
pub fn capture(c: &Value, attempt: &str, retry_incomplete: bool) -> Result<Value> {
    let root = validate(c, true)?;
    if !super::resource::valid_name(attempt) || matches!(attempt, "LAST_SUCCESS" | "lock") {
        return Err(invalid("invalid backup attempt"));
    }
    let protected = c["version"] == 2;
    if protected {
        publication::require_namespace(&root)?;
        publication::check_credential_sources(c.get("credentials"))?;
    }
    let lease = durable::lock(&root.join("lock"), false, false)?;
    let staging = root.join(format!("{attempt}.partial"));
    let partial = if protected {
        staging.join("artifacts")
    } else {
        staging.clone()
    };
    let sealed = root.join(format!("{attempt}.sealed"));
    let credentials = root.join(format!("{attempt}.credentials"));
    let destination = root.join(attempt);
    let intent = root.join(format!("{attempt}.intent.json"));
    let workspace = root.join(format!("{attempt}.restore-workspace"));
    if exists(&destination)
        || fs::symlink_metadata(&staging).is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err(invalid(
            "backup attempt already exists; never overwrite a restore point",
        ));
    }
    let tools = executables(c)?;
    let attempt_intent = json!({"manifest_sha256":identity(c)?,"executables":tools});
    if exists(&intent)
        || exists(&staging)
        || exists(&workspace)
        || (protected && (exists(&sealed) || exists(&credentials)))
    {
        if !retry_incomplete || durable::read_json(&intent)? != attempt_intent {
            return Err(invalid(
                "backup attempt already exists or its immutable intent changed",
            ));
        }
        if exists(&workspace) {
            if !fs::symlink_metadata(&workspace)?.is_dir() {
                return Err(invalid("incomplete restore workspace is redirected"));
            }
            let worker = if protected {
                Some(publication::Worker::new(
                    string(c, "worker_user")?,
                    &root.join(format!("{attempt}.retry-credentials-{}", unique()?)),
                    c.get("credentials"),
                )?)
            } else {
                None
            };
            execute(
                c,
                "cleanup",
                &partial,
                &workspace,
                lease.fd(),
                worker.as_ref(),
            )?;
        }
        if protected && exists(&credentials) {
            fs::remove_dir_all(&credentials)?;
        }
        let abandoned = root.join(format!("{attempt}.abandoned-{}", unique()?));
        private_dir(&abandoned)?;
        let mut abandoned_entries = vec![&staging, &workspace, &intent];
        if protected {
            abandoned_entries.push(&sealed);
        }
        for entry in abandoned_entries {
            if exists(entry) {
                fs::rename(
                    entry,
                    abandoned.join(
                        entry
                            .file_name()
                            .ok_or_else(|| invalid("invalid attempt path"))?,
                    ),
                )?;
            }
        }
        durable::sync_directory(&abandoned)?;
        durable::sync_directory(&root)?;
    }
    durable::write_json(&intent, &attempt_intent)?;
    let worker = if protected {
        Some(publication::Worker::new(
            string(c, "worker_user")?,
            &credentials,
            c.get("credentials"),
        )?)
    } else {
        None
    };
    let captured_at = now()?;
    if let Some(worker) = &worker {
        publication::worker_directory(&staging, &worker.identity)?;
        publication::worker_directory(&workspace, &worker.identity)?;
    } else {
        private_dir(&workspace)?;
    }
    let mut certification_tree = &partial;
    let result = (|| {
        execute(
            c,
            "capture",
            &partial,
            &workspace,
            lease.fd(),
            worker.as_ref(),
        )?;
        capture_receipt(&partial)?;
        let artifacts = inventory(&partial)?;
        if artifacts.get("acceptance.json").is_some() {
            return Err(invalid(
                "application capture must not supply its own acceptance envelope",
            ));
        }
        if protected {
            publication::copy_tree(&partial, &sealed)?;
            publication::reader_permissions(&sealed, fs::metadata(&root)?.gid())?;
            if inventory(&sealed)? != artifacts {
                return Err(invalid(
                    "backup artifact changed before protected certification",
                ));
            }
            certification_tree = &sealed;
        }
        let receipt = certification(
            c,
            certification_tree,
            &workspace,
            lease.fd(),
            &artifacts,
            worker.as_ref(),
        )?;
        Ok((artifacts, receipt))
    })();
    execute(
        c,
        "cleanup",
        certification_tree,
        &workspace,
        lease.fd(),
        worker.as_ref(),
    )?;
    fs::remove_dir_all(&workspace)?;
    durable::sync_directory(&root)?;
    let (artifacts, mut receipt): (Value, Value) = result?;
    if inventory(certification_tree)? != artifacts {
        return Err(invalid("backup artifact hash changed during cleanup"));
    }
    if executables(c)? != tools {
        return Err(invalid("backup executable identity changed during capture"));
    }
    for (k,v) in json!({"resource":c["resource"],"attempt":attempt,"captured_at":captured_at,"manifest_sha256":identity(c)?,"executables":tools}).as_object().unwrap() { receipt[k]=v.clone(); }
    let published = certification_tree;
    durable::write_json(&published.join("acceptance.json"), &receipt)?;
    if protected {
        publication::reader_permissions(published, fs::metadata(&root)?.gid())?;
    } else {
        permissions(published)?;
    }
    durable::publish_tree_checked(published, &destination, || {
        fresh(c, &json!(captured_at), now()?)
    })?;
    durable::write_json_mode(
        &root.join("LAST_SUCCESS"),
        &json!({"attempt":attempt,"acceptance_sha256":codec::file_digest(&destination.join("acceptance.json"))?}),
        0o640,
    )?;
    if protected {
        fs::remove_dir_all(&staging)?;
    }
    Ok(receipt)
}
pub fn verify_bytes(c: &Value, b: &Path, time: Option<i64>) -> Result<Value> {
    let receipt = verify_integrity(c, b)?;
    fresh(c, &receipt["captured_at"], time.unwrap_or(now()?))?;
    Ok(receipt)
}

fn verify_integrity(c: &Value, b: &Path) -> Result<Value> {
    let receipt = durable::read_json(&b.join("acceptance.json"))?;
    if receipt["status"] != "verified"
        || receipt["manifest_sha256"] != identity(c)?
        || receipt["executables"] != executables(c)?
        || receipt["resource"] != c["resource"]
    {
        return Err(invalid(
            "backup acceptance contract or executable identity changed",
        ));
    }
    let mut artifacts = inventory(b)?;
    artifacts.as_object_mut().unwrap().remove("acceptance.json");
    if artifacts != receipt["artifacts"] {
        return Err(invalid("backup artifact hash does not match acceptance"));
    }
    Ok(receipt)
}

fn fresh(c: &Value, captured_at: &Value, now: i64) -> Result<()> {
    let age = now
        .checked_sub(
            captured_at
                .as_i64()
                .ok_or_else(|| invalid("invalid capture time"))?,
        )
        .ok_or_else(|| invalid("invalid backup age"))?;
    if age < 0
        || age
            > c["maximum_age_seconds"]
                .as_i64()
                .ok_or_else(|| invalid("invalid maximum age"))?
    {
        return Err(invalid("backup acceptance is stale or from the future"));
    }
    Ok(())
}
pub fn inspect(c: &Value, b: &Path, time: Option<i64>) -> Result<Value> {
    let root = validate(c, true)?;
    let _lease = durable::lock(&root.join("lock"), true, false)?;
    verify_bytes(c, b, time)
}
/// Retained rollback captures do not expire; their bytes, tools and contract do.
pub fn inspect_retained(c: &Value, b: &Path) -> Result<Value> {
    let root = validate(c, true)?;
    let _lease = durable::lock(&root.join("lock"), true, false)?;
    let receipt = verify_integrity(c, b)?;
    let now = now()?;
    if !receipt["captured_at"]
        .as_i64()
        .is_some_and(|at| at >= 0 && at <= now)
    {
        return Err(invalid(
            "retained backup capture time is invalid or from the future",
        ));
    }
    Ok(receipt)
}
pub fn certify(c: &Value, b: &Path, state: &Path) -> Result<Value> {
    validate(c, false)?;
    if c["version"] == 2 && unsafe { libc::geteuid() } == 0 {
        return Err(invalid(
            "independent backup certification requires an unprivileged account",
        ));
    }
    if !state.is_absolute()
        || !state.is_dir()
        || fs::canonicalize(state)? != state
        || state.starts_with(b)
    {
        return Err(invalid(
            "certifier state must be an existing private directory outside the copied backup",
        ));
    }
    let meta = fs::metadata(state)?;
    if meta.uid() != unsafe { libc::getuid() } || meta.mode() & 0o077 != 0 {
        return Err(invalid("certifier state must be private and owned"));
    }
    let lease = durable::lock(&state.join("lock"), false, false)?;
    let source = verify_bytes(c, b, None)?;
    if source["executor_machine_sha256"] == machine()? || source["executor"] == hostname()? {
        return Err(invalid(
            "independent restoration must execute on a different machine",
        ));
    }
    let workspace = state.join(format!("restore-{}-{}", std::process::id(), unique()?));
    private_dir(&workspace)?;
    let tools = executables(c)?;
    let result = (|| {
        let a = inventory(b)?;
        certification(c, b, &workspace, lease.fd(), &a, None)
    })();
    execute(c, "cleanup", b, &workspace, lease.fd(), None)?;
    fs::remove_dir_all(&workspace)?;
    let mut result = result?;
    verify_bytes(c, b, None)?;
    if executables(c)? != tools {
        return Err(invalid("certifier executable changed during restoration"));
    }
    let hash = codec::file_digest(&b.join("acceptance.json"))?;
    let path = state.join(format!("{hash}.json"));
    let current = now()?;
    let previous = match fs::symlink_metadata(&path) {
        Ok(_) => Some(durable::read_json(&path)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let certified_at = if let Some(previous) = &previous {
        previous["certified_at"]
            .as_i64()
            .filter(|time| {
                source["captured_at"]
                    .as_i64()
                    .is_some_and(|captured| captured <= *time)
                    && *time <= current
            })
            .ok_or_else(|| {
                invalid("independent restore receipt already exists; retain its original evidence")
            })?
    } else {
        current
    };
    for (k,v) in json!({"source_acceptance_sha256":hash,"certified_at":certified_at,"manifest_sha256":identity(c)?,"executables":tools,"resource":c["resource"]}).as_object().unwrap() { result[k]=v.clone(); }
    if let Some(previous) = previous {
        if previous != result {
            return Err(invalid(
                "independent restore receipt already exists; retain its original evidence",
            ));
        }
        // Restore, bytes, tools and the complete receipt were revalidated. Keep
        // its timestamp, inode and bytes when the earlier response was lost.
        return Ok(previous);
    }
    durable::write_json(&path, &result)?;
    Ok(result)
}
