//! Job-scoped indirect GC roots. Registration precedes the durable submission.
use crate::{
    Error, Result,
    contracts::*,
    execution::ExecutionBinding,
    lifecycle::NativeProcess,
    storage::{commit_artifact, private_dir, safe_path},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

fn record(path: &Path, limit: u64) -> Result<Vec<u8>> {
    use std::{io::Read, os::unix::fs::OpenOptionsExt};
    let mut input = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let metadata = input.metadata()?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(invalid("bounded regular retention record required"));
    }
    let mut data = Vec::new();
    input.by_ref().take(limit + 1).read_to_end(&mut data)?;
    if data.len() as u64 != metadata.len() {
        return Err(invalid("retention record changed"));
    }
    Ok(data)
}

pub trait GcRootRegistry {
    fn register(&mut self, root: &Path, target: &Path) -> Result<()>;
}
pub struct NixRoots;
impl GcRootRegistry for NixRoots {
    fn register(&mut self, root: &Path, target: &Path) -> Result<()> {
        // Only already-realized non-derivation store objects are accepted. No
        // evaluation, builds, substitution, configuration changes or uploads.
        let mut command = Command::new(option_env!("HARBOR_CAD_NIX_STORE").unwrap_or("nix-store"));
        command
            .env_clear()
            .env("NIX_REMOTE", "daemon")
            .args(["--realise", "--add-root"])
            .arg(root)
            .args([
                "--indirect",
                "--option",
                "substitute",
                "false",
                "--option",
                "builders",
                "",
                "--max-jobs",
                "0",
            ])
            .arg(target)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let status = NativeProcess::spawn(command)?.wait(Duration::from_secs(15), || Ok(()))?;
        if !status.success() {
            return Err(Error::Resource(
                "job GC root registration failed; runtime must already be realized".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    schema_version: u32,
    job: String,
    binding: ExecutionBinding,
    systemd: bool,
    roots: Vec<PathBuf>,
}

pub(crate) fn store_object(path: &str) -> Result<PathBuf> {
    let path = Path::new(path);
    if !path.starts_with("/nix/store")
        || path.components().any(|p| {
            matches!(
                p,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        })
    {
        return Err(invalid(
            "GC roots require canonical immutable store objects",
        ));
    }
    let object = path
        .strip_prefix("/nix/store")
        .map_err(|_| invalid("store path"))?
        .components()
        .next()
        .ok_or_else(|| invalid("store object"))?
        .as_os_str();
    let name = object
        .to_str()
        .ok_or_else(|| invalid("UTF-8 store object"))?;
    if name.len() < 34
        || name.as_bytes()[32] != b'-'
        || name.ends_with(".drv")
        || !name.as_bytes()[..32]
            .iter()
            .all(|b| b"0123456789abcdfghijklmnpqrsvwxyz".contains(b))
    {
        return Err(invalid("realized non-derivation Nix store object required"));
    }
    Ok(Path::new("/nix/store").join(object))
}
fn roots(binding: &ExecutionBinding, systemd: bool) -> Result<Vec<PathBuf>> {
    if !systemd {
        return Ok(Vec::new());
    }
    let mut paths = BTreeSet::new();
    paths.insert(store_object(&binding.runner.path)?);
    if let Some(runtime) = &binding.native_runtime {
        paths.insert(store_object(&runtime.path)?);
    }
    for file in binding.native_files.values() {
        paths.insert(store_object(&file.path)?);
    }
    if paths.len() > 64 {
        return Err(invalid("bounded runtime root set required"));
    }
    Ok(paths.into_iter().collect())
}
fn directory(root: &Path, id: &str) -> Result<PathBuf> {
    if uuid::Uuid::parse_str(id).is_err() {
        return Err(invalid("retention job UUID required"));
    }
    safe_path(root, &format!("retentions/{id}"))
}
pub fn prepare(root: &Path, id: &str, binding: &ExecutionBinding, systemd: bool) -> Result<()> {
    prepare_with(root, id, binding, systemd, &mut NixRoots)
}
pub fn prepare_with(
    root: &Path,
    id: &str,
    binding: &ExecutionBinding,
    systemd: bool,
    registry: &mut impl GcRootRegistry,
) -> Result<()> {
    let root = fs::canonicalize(root)?;
    let parent = safe_path(&root, "retentions")?;
    private_dir(&parent)?;
    fs::File::open(&root)?.sync_all()?;
    let dir = directory(&root, id)?;
    private_dir(&dir)?;
    fs::File::open(&parent)?.sync_all()?;
    let intent = Intent {
        schema_version: 1,
        job: id.into(),
        binding: binding.clone(),
        systemd,
        roots: roots(binding, systemd)?,
    };
    commit_artifact(
        &dir,
        "intent.json",
        &serde_json::to_vec(&intent)?,
        "json",
        "GC root intent before SQLite commit",
    )?;
    for (index, target) in intent.roots.iter().enumerate() {
        let root = dir.join(format!("root-{index:04}"));
        registry.register(&root, target)?;
        verify_root(&root, target)?;
        fs::File::open(&dir)?.sync_all()?;
    }
    commit_artifact(
        &dir,
        "ready.json",
        &serde_json::to_vec(&serde_json::json!({"binding_digest":digest(binding)?}))?,
        "json",
        "all job GC roots registered before durable submission",
    )?;
    Ok(())
}
fn verify_root(root: &Path, target: &Path) -> Result<()> {
    if !fs::symlink_metadata(root)?.file_type().is_symlink() || fs::read_link(root)? != target {
        return Err(invalid("job GC root target/type mismatch"));
    }
    Ok(())
}
fn read_intent(dir: &Path, id: &str) -> Result<Intent> {
    let intent: Intent =
        serde_json::from_slice(&record(&safe_path(dir, "intent.json")?, MAX_MESSAGE)?)?;
    if intent.schema_version != 1
        || intent.job != id
        || intent.roots != roots(&intent.binding, intent.systemd)?
    {
        return Err(invalid("retention intent identity mismatch"));
    }
    Ok(intent)
}
pub fn verify_ready(
    root: &Path,
    id: &str,
    binding: &ExecutionBinding,
    systemd: bool,
) -> Result<()> {
    let dir = directory(root, id)?;
    if !dir.exists() {
        return Err(invalid("job runtime retention is missing"));
    }
    private_dir(&dir)?;
    let intent = read_intent(&dir, id)?;
    if digest(&intent.binding)? != digest(binding)? || intent.systemd != systemd {
        return Err(invalid(
            "retention differs from immutable execution binding",
        ));
    }
    let ready: serde_json::Value =
        serde_json::from_slice(&record(&safe_path(&dir, "ready.json")?, 1024)?)?;
    if ready["binding_digest"] != digest(binding)? {
        return Err(invalid("retention not durably registered"));
    }
    for (index, target) in intent.roots.iter().enumerate() {
        verify_root(&dir.join(format!("root-{index:04}")), target)?;
    }
    Ok(())
}

/// The caller holds SQLite's writer transaction, preventing cleanup from racing
/// a root registration that has not yet committed its job. Only owned entries
/// are unlinked; targets and the Nix global root registry are never removed.
pub(crate) fn release(root: &Path, id: &str, binding: Option<&ExecutionBinding>) -> Result<()> {
    let dir = directory(root, id)?;
    private_dir(&dir)?;
    let intent = if dir.join("intent.json").exists() {
        Some(read_intent(&dir, id)?)
    } else {
        None
    };
    if let Some(binding) = binding {
        let recorded = intent
            .as_ref()
            .ok_or_else(|| invalid("job retention intent missing"))?;
        if digest(&recorded.binding)? != digest(binding)? {
            return Err(invalid("refusing to release mismatched job retention"));
        }
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| invalid("retention entry"))?;
        if let Some(index) = name
            .strip_prefix("root-")
            .and_then(|n| n.parse::<usize>().ok())
        {
            let target = intent
                .as_ref()
                .and_then(|i| i.roots.get(index))
                .ok_or_else(|| invalid("unowned GC root"))?;
            if name != format!("root-{index:04}") {
                return Err(invalid("GC root name mismatch"));
            }
            verify_root(&entry.path(), target)?;
        } else if (name == "intent.json"
            || name == "ready.json"
            || name
                .strip_prefix(".partial-")
                .is_some_and(|p| uuid::Uuid::parse_str(p).is_ok()))
            && entry.file_type()?.is_file()
        {
            // A crash can leave a closed, unpublished commit_artifact temporary.
        } else {
            return Err(invalid("unexpected entry in job retention directory"));
        }
        entries.push(entry.path());
    }
    // Keep the intent until roots and ready are removed, so interruption during
    // cleanup remains recoverable using the same immutable root targets.
    entries.sort_by_key(|p| p.file_name() == Some(std::ffi::OsStr::new("intent.json")));
    for path in entries {
        fs::remove_file(path)?;
        fs::File::open(&dir)?.sync_all()?;
    }
    fs::remove_dir(&dir)?;
    fs::File::open(dir.parent().ok_or_else(|| invalid("retention parent"))?)?.sync_all()?;
    Ok(())
}
