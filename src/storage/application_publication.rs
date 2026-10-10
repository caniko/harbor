//! Privileged publication copies worker-owned bytes onto protected fresh inodes.
use super::{Result, durable, invalid, process};
use std::{
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
impl Worker {
    pub fn new(user: &str, credentials: &Path) -> Result<Self> {
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
        if let Some(source) = std::env::var_os("CREDENTIALS_DIRECTORY") {
            fs::DirBuilder::new().mode(0o700).create(credentials)?;
            worker.credentials = Some(credentials.to_owned());
            for entry in fs::read_dir(source)? {
                let entry = entry?;
                let mut input = durable::open_regular(&entry.path(), false)?;
                let mut output = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o400)
                    .open(credentials.join(entry.file_name()))?;
                io::copy(&mut input, &mut output)?;
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
            if io::copy(&mut source.take((1 << 30) + 1), &mut output)? > 1 << 30 {
                return Err(invalid(
                    "backup artifact exceeds the complete-file byte limit",
                ));
            }
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
