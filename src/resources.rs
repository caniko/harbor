use crate::{
    Error, Result,
    contracts::invalid,
    storage::{private_dir, safe_path},
};
use fs2::FileExt;
use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

pub fn card_reservation_root() -> Result<PathBuf> {
    let runtime = PathBuf::from(format!("/run/user/{}", unsafe { libc::geteuid() }));
    if !runtime.is_dir() {
        return Err(Error::Unqualified(
            "owned user runtime directory required for shared GPU reservations".into(),
        ));
    }
    private_dir(&runtime)?;
    let application = safe_path(&runtime, "harbor-cad")?;
    private_dir(&application)?;
    Ok(application.join("cards"))
}

fn pci_key(pci: &str) -> Result<String> {
    let bytes = pci.as_bytes();
    if bytes.len() != 12
        || bytes[4] != b':'
        || bytes[7] != b':'
        || bytes[10] != b'.'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, c)| [4, 7, 10].contains(&i) || c.is_ascii_hexdigit())
        || !(b'0'..=b'7').contains(&bytes[11])
    {
        return Err(invalid("canonical PCI domain:bus:device.function required"));
    }
    Ok(pci.to_ascii_lowercase())
}

/// Hold these files in the job process. A busy card releases the entire attempted
/// set; callers wait before starting stages. Anchors are never unlinked.
pub fn try_reserve_cards(root: &Path, pcis: &[String]) -> Result<Option<Vec<File>>> {
    if pcis.is_empty() {
        return Ok(Some(Vec::new()));
    }
    let keys = pcis
        .iter()
        .map(|p| pci_key(p))
        .collect::<Result<BTreeSet<_>>>()?;
    private_dir(root)?;
    let mut held = Vec::new();
    for key in keys {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(safe_path(root, &key)?)?;
        let metadata = file.metadata()?;
        if !metadata.is_file()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.permissions().mode() & 0o077 != 0
            || metadata.nlink() != 1
        {
            return Err(invalid(
                "reservation anchor must be an owned private regular inode",
            ));
        }
        match file.try_lock_exclusive() {
            Ok(()) => held.push(file),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(None),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(Some(held))
}
