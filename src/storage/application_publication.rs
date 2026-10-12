//! Privileged publication copies worker-owned bytes onto protected fresh inodes.
use super::{Result, durable, invalid, process};
use std::{
    collections::VecDeque,
    ffi::CString,
    fs::{self, File, OpenOptions},
    io::{self, Read},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            ffi::OsStrExt,
            fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt, chown},
        },
    },
    path::{Path, PathBuf},
};

/// Bind every alias component through descriptors before reading a privileged
/// source. Neither a worker rename nor a symlink swap can redirect this copy.
fn credential_source(path: &Path) -> Result<File> {
    if !path.is_absolute() {
        return Err(invalid("credential source must be absolute"));
    }
    let root = || -> Result<File> {
        Ok(OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open("/")?)
    };
    let mut current = root()?;
    let mut pending: VecDeque<_> = path
        .components()
        .skip(1)
        .map(|part| part.as_os_str().to_owned())
        .collect();
    let root_uid = durable::root_owner_uid()?;
    let mut links = 0;
    while let Some(name) = pending.pop_front() {
        let parent = current.metadata()?;
        if !parent.is_dir() || parent.uid() != root_uid || parent.mode() & 0o022 != 0 {
            return Err(invalid("untrusted credential source ancestor"));
        }
        let name = CString::new(name.as_bytes())
            .map_err(|_| invalid("invalid credential source component"))?;
        let fd = unsafe {
            libc::openat(
                current.as_raw_fd(),
                name.as_ptr(),
                libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error().into());
        }
        let selected = unsafe { File::from_raw_fd(fd) };
        let meta = selected.metadata()?;
        if meta.uid() != root_uid || (!meta.file_type().is_symlink() && meta.mode() & 0o022 != 0) {
            return Err(invalid(
                "untrusted credential source ownership or permissions",
            ));
        }
        if meta.file_type().is_symlink() {
            links += 1;
            if links > 40 {
                return Err(invalid("credential source has too many alias hops"));
            }
            let mut target = vec![0u8; 4097];
            let length = unsafe {
                libc::readlinkat(
                    selected.as_raw_fd(),
                    c"".as_ptr(),
                    target.as_mut_ptr().cast(),
                    target.len(),
                )
            };
            if length < 0 {
                return Err(io::Error::last_os_error().into());
            }
            if length as usize == target.len() {
                return Err(invalid("credential alias target is too long"));
            }
            target.truncate(length as usize);
            let target = Path::new(std::ffi::OsStr::from_bytes(&target));
            if target.is_absolute() {
                current = root()?;
            }
            let mut parts: VecDeque<_> = target
                .components()
                .filter(|part| !matches!(part, std::path::Component::RootDir))
                .map(|part| part.as_os_str().to_owned())
                .collect();
            parts.append(&mut pending);
            pending = parts;
        } else {
            current = selected;
        }
    }
    let meta = current.metadata()?;
    if !meta.is_file() || meta.uid() != root_uid || meta.mode() & 0o022 != 0 {
        return Err(invalid("credential source must be a trusted regular file"));
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(format!("/proc/self/fd/{}", current.as_raw_fd()))?;
    let opened = file.metadata()?;
    if (opened.dev(), opened.ino()) != (meta.dev(), meta.ino()) {
        return Err(invalid("credential source inode changed"));
    }
    Ok(file)
}

pub(super) fn require_namespace(root: &Path) -> Result<()> {
    if unsafe { libc::geteuid() } != 0 {
        return Err(invalid(
            "protected backup publication requires a root coordinator",
        ));
    }
    for (index, path) in root.ancestors().enumerate() {
        let meta = fs::symlink_metadata(path)?;
        let sticky = index != 0 && meta.mode() & libc::S_ISVTX != 0;
        if !meta.is_dir() || meta.uid() != 0 || (meta.mode() & 0o022 != 0 && !sticky) {
            return Err(invalid(
                "protected backup namespace has untrusted ownership or permissions",
            ));
        }
    }
    let lock = fs::symlink_metadata(root.join("lock"))?;
    if !lock.is_file() || lock.uid() != 0 || lock.nlink() != 1 || lock.mode() & 0o022 != 0 {
        return Err(invalid("protected backup lease is not root-owned"));
    }
    Ok(())
}

pub(super) fn worker_directory(path: &Path, identity: &process::Identity) -> Result<()> {
    fs::DirBuilder::new().mode(0o700).create(path)?;
    // The parent is protected, and this new directory stays root-private until
    // the final ownership transfer. No worker-controlled pathname is followed.
    chown(path, Some(identity.uid), Some(identity.gid))?;
    Ok(())
}

pub(super) struct Worker {
    pub identity: process::Identity,
    pub credentials: Option<PathBuf>,
}
pub(super) fn validate_credentials(sources: &serde_json::Value) -> Result<()> {
    let sources = sources
        .as_object()
        .ok_or_else(|| invalid("invalid backup credential sources"))?;
    for (name, source) in sources {
        if name.is_empty()
            || name.len() > 255
            || matches!(name.as_str(), "." | "..")
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        {
            return Err(invalid("invalid backup credential name"));
        }
        let source = Path::new(
            source
                .as_str()
                .ok_or_else(|| invalid("invalid backup credential source"))?,
        );
        if !source.is_absolute() || source.starts_with("/nix/store") {
            return Err(invalid(
                "backup credentials require runtime paths outside the store",
            ));
        }
    }
    Ok(())
}
impl Worker {
    pub fn new(
        user: &str,
        credentials: &Path,
        sources: Option<&serde_json::Value>,
    ) -> Result<Self> {
        let identity = process::account(user)?;
        if identity.uid == 0 {
            return Err(invalid(
                "backup adapters require an unprivileged worker account",
            ));
        }
        let mut worker = Self {
            identity,
            credentials: None,
        };
        let inputs = credential_inputs(sources)?;
        if !inputs.is_empty() {
            fs::DirBuilder::new().mode(0o700).create(credentials)?;
            worker.credentials = Some(credentials.to_owned());
            for (name, input) in inputs {
                let mut output = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o400)
                    .open(credentials.join(name))?;
                if io::copy(&mut input.take((1 << 20) + 1), &mut output)? > 1 << 20 {
                    return Err(invalid("backup credential exceeds the byte limit"));
                }
                // Use the opened destination, not a worker-controlled alias.
                let status = unsafe {
                    libc::fchown(output.as_raw_fd(), worker.identity.uid, worker.identity.gid)
                };
                if status != 0 {
                    return Err(io::Error::last_os_error().into());
                }
            }
            chown(
                credentials,
                Some(worker.identity.uid),
                Some(worker.identity.gid),
            )?;
        }
        Ok(worker)
    }
}

fn credential_inputs(sources: Option<&serde_json::Value>) -> Result<Vec<(String, File)>> {
    let inherited = std::env::var_os("CREDENTIALS_DIRECTORY").map(PathBuf::from);
    let mut selected = Vec::new();
    if let Some(sources) = sources {
        validate_credentials(sources)?;
        for (name, source) in sources.as_object().unwrap() {
            let path = inherited.as_ref().map_or_else(
                || PathBuf::from(source.as_str().unwrap()),
                |directory| directory.join(name),
            );
            selected.push((name.clone(), path));
        }
    } else if let Some(directory) = inherited {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            selected.push((
                entry
                    .file_name()
                    .into_string()
                    .map_err(|_| invalid("invalid backup credential name"))?,
                entry.path(),
            ));
        }
    }
    selected
        .into_iter()
        .map(|(name, path)| {
            let file = credential_source(&path)?;
            if file.metadata()?.len() > 1 << 20 {
                return Err(invalid("backup credential exceeds the byte limit"));
            }
            Ok((name, file))
        })
        .collect()
}

pub(super) fn check_credential_sources(sources: Option<&serde_json::Value>) -> Result<()> {
    credential_inputs(sources)?;
    Ok(())
}
impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(path) = &self.credentials {
            let _ = fs::remove_dir_all(path);
        }
    }
}

fn open_child(parent: &File, name: &std::ffi::OsStr) -> Result<File> {
    let name = CString::new(name.as_bytes()).map_err(|_| invalid("invalid artifact name"))?;
    // Open each component relative to an already opened directory. A staging
    // rename/symlink race cannot redirect a privileged copy outside that tree.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error().into());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}
fn copy_directory(input: &File, destination: &Path) -> Result<()> {
    for entry in fs::read_dir(format!("/proc/self/fd/{}", input.as_raw_fd()))? {
        let name = entry?.file_name();
        let source = open_child(input, &name)?;
        let meta = source.metadata()?;
        let target = destination.join(name);
        if meta.is_dir() {
            fs::DirBuilder::new().mode(0o700).create(&target)?;
            copy_directory(&source, &target)?;
        } else if meta.is_file() && meta.nlink() == 1 {
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(target)?;
            io::copy(&mut &source, &mut output)?;
        } else {
            return Err(invalid(
                "backup contains redirected, shared or special files",
            ));
        }
    }
    Ok(())
}
pub(super) fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    let input = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(source)?;
    fs::DirBuilder::new().mode(0o700).create(destination)?;
    copy_directory(&input, destination)
}
pub(super) fn reader_permissions(path: &Path, gid: u32) -> Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            reader_permissions(&entry.path(), gid)?;
        } else {
            chown(entry.path(), Some(0), Some(gid))?;
            fs::set_permissions(entry.path(), fs::Permissions::from_mode(0o440))?;
        }
    }
    chown(path, Some(0), Some(gid))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o750))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Seek, SeekFrom, Write};

    #[test]
    fn worker_credentials_reject_untrusted_sources_before_creating_private_copies_in_both_engines()
    {
        use serde_json::json;
        assert!(std::env::var_os("CREDENTIALS_DIRECTORY").is_none());
        let user = super::super::login_shell::current_user().unwrap();
        let account = process::account(&user).unwrap();
        assert_ne!(account.uid, 0);
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("worker-secret");
        fs::write(&source, b"confidential source bytes").unwrap();
        let copied = root.path().join("credentials");
        let sources = json!({"token":source});
        let result = Worker::new(&user, &copied, Some(&sources));
        assert!(
            result.is_err(),
            "worker-controlled credential source was copied"
        );
        assert!(
            !copied.exists(),
            "credential rejection mutated the copy namespace"
        );
        let script = r#"import os,sys
from pathlib import Path
from harbor_db.application_publication import Worker
root=Path(sys.argv[2]);copied=root/'python-credentials'
try: Worker(sys.argv[1],copied,{'token':str(root/'worker-secret')})
except (ValueError,OSError): pass
else: raise AssertionError('worker-controlled credential source was copied')
assert not copied.exists(),'credential rejection mutated the copy namespace'
assert (root/'worker-secret').read_bytes()==b'confidential source bytes'
"#;
        let mut spec = process::CommandSpec::new(vec![
            "python3".into(),
            "-B".into(),
            "-c".into(),
            script.into(),
            user,
            root.path().display().to_string(),
        ]);
        spec.environment = Some(std::collections::BTreeMap::from([
            (
                "PYTHONPATH".into(),
                format!("{}/python", env!("CARGO_MANIFEST_DIR")),
            ),
            ("PATH".into(), std::env::var("PATH").unwrap()),
        ]));
        process::execute(&spec).unwrap();
    }

    #[test]
    fn credential_sources_require_bounded_names_and_runtime_paths_in_both_engines() {
        use serde_json::json;
        for (sources, accepted) in [
            (json!({}), true),
            (json!({"A_0-z.token":"/run/fixture-secret"}), true),
            (json!({"a".repeat(255):"/run/fixture-secret"}), true),
            (json!({"a".repeat(256):"/run/fixture-secret"}), false),
            (json!({"":"/run/fixture-secret"}), false),
            (json!({".":"/run/fixture-secret"}), false),
            (json!({"..":"/run/fixture-secret"}), false),
            (json!({"../token":"/run/fixture-secret"}), false),
            (json!({"token":"relative-secret"}), false),
            (json!({"token":"/nix/store/secret"}), false),
            (json!({"token":null}), false),
            (json!([]), false),
            (json!(null), false),
        ] {
            assert_eq!(validate_credentials(&sources).is_ok(), accepted);
            let script = r#"import json,sys
from harbor_db.application_publication import validate_credentials
try: validate_credentials(json.loads(sys.argv[1]))
except ValueError: accepted=False
else: accepted=True
assert accepted == json.loads(sys.argv[2])
"#;
            let mut spec = process::CommandSpec::new(vec![
                "python3".into(),
                "-B".into(),
                "-c".into(),
                script.into(),
                sources.to_string(),
                accepted.to_string(),
            ]);
            spec.environment = Some(std::collections::BTreeMap::from([
                (
                    "PYTHONPATH".into(),
                    format!("{}/python", env!("CARGO_MANIFEST_DIR")),
                ),
                ("PATH".into(), std::env::var("PATH").unwrap()),
            ]));
            process::execute(&spec).unwrap();
        }
    }

    #[test]
    fn sealed_copy_is_independent_of_retained_worker_write_handles_in_both_engines() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("staging");
        fs::create_dir_all(source.join("required-empty/nested")).unwrap();
        fs::write(source.join("records"), b"retained revision seven").unwrap();
        let mut writer = OpenOptions::new()
            .write(true)
            .open(source.join("records"))
            .unwrap();
        let native = root.path().join("native");
        copy_tree(&source, &native).unwrap();
        let script = r#"import os,sys
from pathlib import Path
from harbor_db import application_publication
source,destination=map(Path,sys.argv[1:])
application_publication.copy_tree(source,destination)
assert os.stat(source/'records').st_ino != os.stat(destination/'records').st_ino
"#;
        let peer = root.path().join("peer");
        let mut spec = process::CommandSpec::new(vec![
            "python3".into(),
            "-B".into(),
            "-c".into(),
            script.into(),
            source.to_string_lossy().into_owned(),
            peer.to_string_lossy().into_owned(),
        ]);
        spec.environment = Some(std::collections::BTreeMap::from([
            (
                "PYTHONPATH".into(),
                format!("{}/python", env!("CARGO_MANIFEST_DIR")),
            ),
            ("PATH".into(), std::env::var("PATH").unwrap()),
        ]));
        process::execute(&spec).unwrap();
        writer.seek(SeekFrom::Start(0)).unwrap();
        writer
            .write_all(b"changed through retained write fd")
            .unwrap();
        fs::rename(source.join("records"), source.join("replaced")).unwrap();
        fs::write(source.join("records"), b"replacement inode").unwrap();
        for published in [&native, &peer] {
            assert_eq!(
                fs::read(published.join("records")).unwrap(),
                b"retained revision seven"
            );
            assert!(published.join("required-empty/nested").is_dir());
            assert_ne!(
                fs::metadata(published.join("records")).unwrap().ino(),
                writer.metadata().unwrap().ino()
            );
        }
    }

    #[test]
    fn sealed_copy_streams_complete_large_artifacts_in_both_engines() {
        use super::super::codec;
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("staging");
        fs::create_dir(&source).unwrap();
        let artifact = source.join("database.dump");
        let mut file = File::create(&artifact).unwrap();
        file.set_len((1 << 30) + 65536).unwrap();
        file.write_all(b"archive header").unwrap();
        file.seek(SeekFrom::Start((1 << 30) + 32768)).unwrap();
        file.write_all(b"record beyond the former limit").unwrap();
        drop(file);
        let expected = codec::file_digest(&artifact).unwrap();
        let mut results = vec![];
        for python in [false, true] {
            let destination = root.path().join(if python { "python" } else { "native" });
            let accepted = if python {
                let mut spec = process::CommandSpec::new(vec!["python3".into(),"-B".into(),"-c".into(),"from pathlib import Path;import sys;from harbor_db.application_publication import copy_tree;copy_tree(*map(Path,sys.argv[1:]))".into(),source.display().to_string(),destination.display().to_string()]);
                spec.environment = Some(std::collections::BTreeMap::from([
                    (
                        "PYTHONPATH".into(),
                        format!("{}/python", env!("CARGO_MANIFEST_DIR")),
                    ),
                    ("PATH".into(), std::env::var("PATH").unwrap()),
                ]));
                spec.timeout = std::time::Duration::from_secs(120);
                process::execute(&spec).is_ok()
            } else {
                copy_tree(&source, &destination).is_ok()
            };
            if accepted {
                let copied = destination.join("database.dump");
                assert_eq!(
                    fs::metadata(&copied).unwrap().len(),
                    fs::metadata(&artifact).unwrap().len()
                );
                assert_eq!(codec::file_digest(&copied).unwrap(), expected);
                assert_ne!(
                    fs::metadata(&copied).unwrap().ino(),
                    fs::metadata(&artifact).unwrap().ino()
                );
            }
            results.push((python, accepted));
            fs::remove_dir_all(destination).unwrap();
        }
        assert!(results.iter().all(|(_, accepted)| *accepted), "{results:?}");
    }

    #[test]
    fn sealed_copy_rejects_redirected_shared_and_special_inputs_in_both_engines() {
        for kind in ["symlink", "hardlink", "fifo"] {
            let root = tempfile::tempdir().unwrap();
            let source = root.path().join("staging");
            fs::create_dir(&source).unwrap();
            let secret = root.path().join("external");
            fs::write(&secret, b"outside staged artifacts").unwrap();
            let path = source.join("records");
            match kind {
                "symlink" => std::os::unix::fs::symlink(&secret, &path).unwrap(),
                "hardlink" => fs::hard_link(&secret, &path).unwrap(),
                _ => {
                    let name = CString::new(path.as_os_str().as_bytes()).unwrap();
                    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
                }
            }
            assert!(copy_tree(&source, &root.path().join("native")).is_err());
            let script = r#"import sys
from pathlib import Path
from harbor_db import application_publication
try: application_publication.copy_tree(Path(sys.argv[1]),Path(sys.argv[2]))
except (ValueError,OSError): pass
else: raise AssertionError('unsafe source copied')
"#;
            let mut spec = process::CommandSpec::new(vec![
                "python3".into(),
                "-B".into(),
                "-c".into(),
                script.into(),
                source.to_string_lossy().into_owned(),
                root.path().join("peer").to_string_lossy().into_owned(),
            ]);
            spec.environment = Some(std::collections::BTreeMap::from([
                (
                    "PYTHONPATH".into(),
                    format!("{}/python", env!("CARGO_MANIFEST_DIR")),
                ),
                ("PATH".into(), std::env::var("PATH").unwrap()),
            ]));
            process::execute(&spec).unwrap();
            assert_eq!(fs::read(secret).unwrap(), b"outside staged artifacts");
        }
    }
}
