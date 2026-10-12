use std::{
    fs::{self, File},
    io::Write,
    path::Path,
};

use anyhow::{Context, Result};
use serde::{Serialize, de::DeserializeOwned};

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    serde_json::from_reader(
        File::open(path).with_context(|| format!("opening {}", path.display()))?,
    )
    .with_context(|| format!("parsing {}", path.display()))
}

/// Write complete private JSON beside its destination, sync it, then rename.
/// With `replace=false`, an existing receipt is never overwritten. Evidence
/// directories and storage/preservation policy belong to the caller.
pub fn write_json_atomic(path: &Path, value: &impl Serialize, replace: bool) -> Result<()> {
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).context("creating atomic receipt")?;
    serde_json::to_writer_pretty(temporary.as_file_mut(), value).context("serializing receipt")?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    if replace {
        temporary.persist(path)?;
    } else {
        temporary.persist_noclobber(path)?;
    }
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    // Retain the destination after a directory sync error: the effect is unknown,
    // not a reason to replay acceptance or silently overwrite a receipt.
    ensure_regular(path)
}

fn ensure_regular(path: &Path) -> Result<()> {
    anyhow::ensure!(
        fs::symlink_metadata(path)?.is_file(),
        "receipt is not a regular file"
    );
    Ok(())
}
