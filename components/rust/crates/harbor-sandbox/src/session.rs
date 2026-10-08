use crate::{Profile, validate_name};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct Session {
    pub(crate) profile: Profile,
    pub(crate) source: PathBuf,
    pub(crate) directory: PathBuf,
    pub(crate) build: PathBuf,
    pub(crate) runtime: PathBuf,
    pub(crate) environment_hash: String,
    pub(crate) variant: String,
    pub(crate) _lock: File,
}

pub struct Snapshot {
    pub workspace: PathBuf,
    pub source_hash: String,
    pub(crate) locks: Vec<(PathBuf, Vec<u8>)>,
}

impl Snapshot {
    pub(crate) fn verify_locks(&self) -> Result<()> {
        for (path, bytes) in &self.locks {
            let mut ancestor = self.workspace.clone();
            for component in path.strip_prefix(&self.workspace)?.components() {
                ancestor.push(component);
                ensure!(
                    !fs::symlink_metadata(&ancestor)?.file_type().is_symlink(),
                    "build replaced a lockfile ancestor with a symlink"
                );
            }
            ensure!(
                fs::read(path)? == *bytes,
                "build changed a lockfile: {}",
                path.display()
            );
        }
        Ok(())
    }
}

impl Session {
    pub fn open(profile: &Profile, source: &Path, state: &Path, variant: &str) -> Result<Self> {
        ensure!(
            cfg!(target_os = "linux"),
            "Harbor development sandboxes require Linux"
        );
        profile.validate()?;
        validate_name(variant)?;
        let source = source.canonicalize().context("resolve source checkout")?;
        ensure!(source.is_dir(), "source is not a directory");
        let prospective_state = resolve_new_path(state)?;
        ensure!(
            !prospective_state.starts_with(&source) && !source.starts_with(&prospective_state),
            "state and source must not overlap"
        );
        private_dir(state)?;
        let state = state.canonicalize()?;
        ensure!(
            !state.starts_with(&source) && !source.starts_with(&state),
            "state and source must not overlap"
        );
        let source_id = hex_hash(source.as_os_str().as_encoded_bytes());
        let directory = state.join(format!("{}-{}-{variant}", profile.name, &source_id[..16]));
        private_dir(&directory)?;
        let lock = lock_file(&directory)?;
        lock.try_lock()
            .context("sandbox variant already has a supervisor")?;
        let environment_hash = hex_hash(&serde_json::to_vec(&serde_json::json!({
            "tools": profile.tools, "environment": profile.environment,
            "shellHook": profile.shell_hook, "cargoConfig": profile.cargo_config,
            "build": profile.build, "desktop": profile.desktop,
            "network": profile.network, "gpu": profile.gpu,
        }))?);
        let build = directory.join("build").join(&environment_hash);
        let runtime = directory.join("runtime");
        for path in [&build, &runtime, &directory.join("generations")] {
            private_dir(path)?;
        }
        for name in [
            "home",
            "config",
            "data",
            "cache",
            "state",
            "run",
            "tmp",
            "artifacts",
        ] {
            private_dir(&runtime.join(name))?;
        }
        for name in ["target", "cargo"] {
            private_dir(&build.join(name))?;
        }
        if let Some(config) = &profile.cargo_config {
            let destination = build.join("cargo/config.toml");
            if let Ok(metadata) = fs::symlink_metadata(&destination) {
                ensure!(
                    metadata.is_file(),
                    "Cargo config destination must not be a symlink"
                );
            }
            fs::copy(config, build.join("cargo/config.toml"))?;
        }
        Ok(Self {
            profile: profile.clone(),
            source,
            directory,
            build,
            runtime,
            environment_hash,
            variant: variant.into(),
            _lock: lock,
        })
    }

    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn source_hash(&self) -> Result<String> {
        let mut hash = Sha256::new();
        for (relative, path) in self.files()? {
            hash.update(relative.as_os_str().as_encoded_bytes());
            hash.update([0]);
            hash_file(&mut hash, &path)?;
        }
        Ok(format!("{:x}", hash.finalize()))
    }

    /// Cheap change detection avoids re-reading large unchanged design assets on
    /// every poll. A changed stamp still requires a full content hash/snapshot.
    pub(crate) fn source_stamp(&self) -> Result<String> {
        let mut hash = Sha256::new();
        for (relative, path) in self.files()? {
            let metadata = fs::metadata(path)?;
            hash.update(relative.as_os_str().as_encoded_bytes());
            hash.update([0]);
            hash.update(metadata.len().to_le_bytes());
            hash.update(metadata.mtime().to_le_bytes());
            hash.update(metadata.mtime_nsec().to_le_bytes());
            hash.update(metadata.ctime().to_le_bytes());
            hash.update(metadata.ctime_nsec().to_le_bytes());
            hash.update(metadata.mode().to_le_bytes());
        }
        Ok(format!("{:x}", hash.finalize()))
    }

    pub fn snapshot(&self, generation: u64) -> Result<Snapshot> {
        let workspace = self
            .directory
            .join("generations")
            .join(generation.to_string())
            .join("workspace");
        ensure!(!workspace.exists(), "generation already exists");
        private_dir(&workspace)?;
        let mut hash = Sha256::new();
        let mut locks = Vec::new();
        for (relative, path) in self.files()? {
            let destination = workspace.join(&relative);
            if path.is_dir() {
                private_dir(&destination)?;
            } else {
                private_dir(destination.parent().context("input parent")?)?;
                fs::copy(&path, &destination)?;
                // Strip special permission bits, retaining executable fixtures/scripts.
                let mode = fs::metadata(&path)?.permissions().mode();
                fs::set_permissions(
                    &destination,
                    fs::Permissions::from_mode(0o600 | (mode & 0o111)),
                )?;
                if matches!(
                    relative.file_name().and_then(|s| s.to_str()),
                    Some("Cargo.lock" | "flake.lock")
                ) {
                    locks.push((destination.clone(), fs::read(&destination)?));
                }
            }
            hash.update(relative.as_os_str().as_encoded_bytes());
            hash.update([0]);
            hash_file(&mut hash, &destination)?;
        }
        Ok(Snapshot {
            workspace,
            source_hash: format!("{:x}", hash.finalize()),
            locks,
        })
    }

    fn files(&self) -> Result<Vec<(PathBuf, PathBuf)>> {
        let mut result = Vec::new();
        for input in &self.profile.inputs {
            let mut ancestor = self.source.clone();
            for component in input.components() {
                ancestor.push(component);
                ensure!(
                    !fs::symlink_metadata(&ancestor)?.file_type().is_symlink(),
                    "symlink source input: {}",
                    ancestor.display()
                );
            }
            for entry in WalkDir::new(self.source.join(input))
                .sort_by_file_name()
                .follow_links(false)
            {
                let entry = entry?;
                let kind = entry.file_type();
                ensure!(
                    kind.is_file() || kind.is_dir(),
                    "source inputs must be regular files/directories: {}",
                    entry.path().display()
                );
                result.push((
                    entry.path().strip_prefix(&self.source)?.to_owned(),
                    entry.path().to_owned(),
                ));
            }
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(result)
    }
}

fn hash_file(hash: &mut Sha256, path: &Path) -> Result<()> {
    if path.is_dir() {
        hash.update(b"directory");
        return Ok(());
    }
    hash.update(b"file");
    hash.update((fs::metadata(path)?.permissions().mode() & 0o111).to_le_bytes());
    let mut file = File::open(path)?;
    let mut buffer = vec![0; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    // Delimit content, including its length, to make file-tree hashing unambiguous.
    hash.update(fs::metadata(path)?.len().to_le_bytes());
    Ok(())
}

pub(crate) fn hex_hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn resolve_new_path(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    ensure!(
        !absolute
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir)),
        "state path must not contain '..'"
    );
    let mut ancestor = absolute.as_path();
    while !ancestor.exists() {
        ancestor = ancestor
            .parent()
            .context("state path has no existing ancestor")?;
    }
    Ok(ancestor
        .canonicalize()?
        .join(absolute.strip_prefix(ancestor)?))
}

pub(crate) fn private_dir(path: &Path) -> Result<()> {
    if !path.exists() {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty())
            && !parent.exists()
        {
            private_dir(parent)?;
        }
        fs::create_dir(path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "state directory must not be a symlink: {}",
        path.display()
    );
    ensure!(
        metadata.uid() == fs::metadata("/proc/self")?.uid() && metadata.mode() & 0o777 == 0o700,
        "state directory must be owned by the caller and mode 0700: {}",
        path.display()
    );
    Ok(())
}

pub(crate) fn lock_file(directory: &Path) -> Result<File> {
    let path = directory.join("supervisor.lock");
    if path.exists() {
        ensure!(
            fs::symlink_metadata(&path)?.is_file(),
            "invalid lock anchor"
        );
    }
    Ok(OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)?)
}

pub(crate) fn atomic_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    let temp = path.with_extension(format!(
        "json.{}.{}.new",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)?;
    let result = (|| {
        file.write_all(&serde_json::to_vec_pretty(value)?)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}
