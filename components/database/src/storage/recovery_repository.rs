//! Additive source-local capture selection. Callers retain the repository lease.
use super::{Result, codec, durable, invalid, recovery, string, writer_fence};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn source_local(settings: &Value) -> Result<bool> {
    match settings.get("repository_protocol") {
        None => Ok(false),
        Some(Value::String(value)) if value == "legacy" => Ok(false),
        Some(Value::String(value)) if value == "source-local-v1" => Ok(true),
        _ => Err(invalid("invalid recovery repository protocol")),
    }
}

pub fn valid_identifier(identifier: &str) -> bool {
    !identifier.is_empty()
        && identifier.len() <= 128
        && identifier.as_bytes()[0].is_ascii_alphanumeric()
        && identifier
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        && !identifier.ends_with(".partial")
}

fn bounded_carrier(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    durable::open_regular(&recovery::absolute(path)?, false)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(invalid(
            "recovery repository carrier exceeds its byte limit",
        ));
    }
    Ok(bytes)
}

pub fn reject_retired(root: &Path, identifier: &str) -> Result<()> {
    match fs::symlink_metadata(
        root.join("recovery/captures")
            .join(format!("{identifier}.retired")),
    ) {
        Ok(_) => Err(invalid(
            "capture identifier is permanently retired; use a new identifier",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn optional_carrier(path: &Path, limit: u64) -> Result<Option<Vec<u8>>> {
    match fs::symlink_metadata(path) {
        Ok(_) => bounded_carrier(path, limit).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Explicit operator retirement. The exclusive repository lease excludes all
/// capture/restore readers; the immutable tombstone makes interrupted unlinking
/// resumable and prevents the identifier from ever being republished.
pub fn retire(config: &Value, identifier: &str) -> Result<Value> {
    let settings = config
        .get("recovery")
        .ok_or_else(|| invalid("missing recovery policy"))?;
    if !source_local(settings)? || !valid_identifier(identifier) {
        return Err(invalid(
            "retirement requires source-local-v1 and a valid capture identifier",
        ));
    }
    let root = recovery::absolute(Path::new(string(settings, "backup_root")?))?;
    let major = writer_fence::major(config)?;
    let system = string(settings, "system_identifier")?;
    if !writer_fence::valid_identifier(system)
        || major.is_empty()
        || !major.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(invalid("invalid retirement repository identity"));
    }
    let local = root.join("recovery");
    for name in ["pins", "captures", "snapshots"] {
        let directory = recovery::absolute(&local.join(name))?;
        if !fs::symlink_metadata(directory)?.is_dir() {
            return Err(invalid(
                "retirement requires existing repository directories",
            ));
        }
    }
    let _lease = durable::lock(
        &recovery::absolute(&root.join("locks/mutate"))?,
        false,
        false,
    )?;
    if std::str::from_utf8(&bounded_carrier(&local.join("PROTOCOL"), 256)?)
        .map_err(|_| invalid("invalid recovery repository protocol"))?
        .trim()
        != "source-local-v1"
    {
        return Err(invalid("invalid recovery repository protocol"));
    }
    if let Some(selected) = optional_carrier(&local.join("SELECTED"), 256)? {
        let selected = std::str::from_utf8(&selected)
            .map_err(|_| invalid("invalid capture selector"))?
            .trim();
        if !valid_identifier(selected) || selected == identifier {
            return Err(invalid(
                "cannot retire the selected capture or an uncertain selection",
            ));
        }
    }
    let names = [
        format!("captures/{identifier}.json"),
        format!("captures/{identifier}.wal-inventory"),
        format!("snapshots/{identifier}.json"),
        format!("pins/{identifier}.json"),
    ];
    let carriers = names
        .iter()
        .map(|name| optional_carrier(&local.join(name), 16 << 20))
        .collect::<Result<Vec<_>>>()?;
    let journal = local.join("captures").join(format!("{identifier}.retired"));
    let header = json!({"version":1,"capture_id":identifier,"backup_root":root,"major":major,"system_identifier":system});
    let record = if let Some(bytes) = optional_carrier(&journal, 16 << 20)? {
        let record = codec::decode(&bytes)?;
        let mut saved = record.clone();
        let hashes = saved
            .as_object_mut()
            .ok_or_else(|| invalid("invalid capture retirement journal"))?
            .remove("carriers")
            .ok_or_else(|| invalid("missing retirement carriers"))?;
        if saved != header
            || hashes
                .as_object()
                .is_none_or(|hashes| hashes.len() != names.len())
            || names.iter().any(|name| hashes.get(name).is_none())
            || hashes[&names[3]].is_null()
        {
            return Err(invalid(
                "capture retirement journal differs from repository identity",
            ));
        }
        for (name, current) in names.iter().zip(&carriers) {
            let expected = &hashes[name];
            if (!expected.is_null()
                && !expected.as_str().is_some_and(|hash| {
                    hash.len() == 64
                        && hash
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                }))
                || current
                    .as_ref()
                    .is_some_and(|bytes| *expected != codec::digest(bytes))
            {
                return Err(invalid(
                    "capture retirement carrier changed; preserving remaining evidence",
                ));
            }
        }
        record
    } else {
        let pin = carriers[3]
            .as_ref()
            .ok_or_else(|| invalid("capture retirement requires an existing pin"))?;
        let metadata = codec::decode(pin)?;
        if metadata["version"] != 1
            || metadata["capture_id"] != identifier
            || metadata["system_identifier"] != system
            || writer_fence::major(&json!({"major":metadata["pg_major"]}))? != major
            || !metadata["backup_id"].as_str().is_some_and(valid_identifier)
            || carriers[0].as_ref().is_some_and(|bytes| bytes != pin)
        {
            return Err(invalid(
                "capture retirement pin or metadata differs from repository identity",
            ));
        }
        if let Some(bytes) = &carriers[2]
            && codec::decode(bytes)?["metadata_sha256"] != codec::digest(pin)
        {
            return Err(invalid("capture retirement snapshot differs from its pin"));
        }
        let hashes = names
            .iter()
            .zip(&carriers)
            .map(|(name, bytes)| {
                (
                    name.clone(),
                    bytes
                        .as_ref()
                        .map_or(Value::Null, |bytes| json!(codec::digest(bytes))),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        let mut record = header;
        record["carriers"] = json!(hashes);
        durable::write_json(&journal, &record)?;
        record
    };
    // Retain the journal if producer contention interrupts retirement. A retry
    // cannot rebind the generation after its retirement has been acknowledged.
    let _backup = durable::lock(
        &recovery::absolute(&root.join("BACKUP_LOCK"))?,
        false,
        false,
    )?;
    // The pin is removed last, after each obsolete evidence unlink is durable.
    for (name, bytes) in names.iter().zip(&carriers) {
        if bytes.is_some() {
            let path = local.join(name);
            fs::remove_file(&path)?;
        }
        // Also flush absent carriers: a previous attempt may have unlinked
        // one and died before persisting that parent directory.
        durable::sync_directory(
            local
                .join(name)
                .parent()
                .ok_or_else(|| invalid("invalid retirement path"))?,
        )?;
    }
    Ok(record)
}

/// Frozen decoded metadata and byte-derived binding; no publication authority.
pub struct SelectedCapture {
    directory: PathBuf,
    metadata_path: PathBuf,
    snapshot_path: PathBuf,
    metadata: Value,
    binding: Value,
}
impl SelectedCapture {
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn metadata_path(&self) -> &Path {
        &self.metadata_path
    }
    pub fn snapshot_path(&self) -> &Path {
        &self.snapshot_path
    }
    pub fn metadata(&self) -> &Value {
        &self.metadata
    }
    pub fn binding(&self) -> &Value {
        &self.binding
    }
    pub fn validate_snapshot(&self, token: Option<&str>, records: &Value) -> Result<()> {
        if token != self.metadata["writer_fence_token"].as_str()
            || records != &self.metadata["record_hashes"]
        {
            return Err(invalid(
                "source-local capture differs from the live fenced records",
            ));
        }
        Ok(())
    }
}

pub fn select(config: &Value, settings: &Value, now: i64) -> Result<SelectedCapture> {
    if !source_local(settings)? {
        return Err(invalid(
            "source-local capture selection requires source-local-v1",
        ));
    }
    let root = recovery::absolute(Path::new(string(settings, "backup_root")?))?;
    let protocol = bounded_carrier(&root.join("recovery/PROTOCOL"), 256)?;
    if std::str::from_utf8(&protocol)
        .map_err(|_| invalid("invalid recovery repository protocol"))?
        .trim()
        != "source-local-v1"
    {
        return Err(invalid("invalid recovery repository protocol"));
    }
    let selector = recovery::absolute(&root.join("recovery/SELECTED"))?;
    let mut bytes = Vec::new();
    durable::open_regular(&selector, false)?
        .take(257)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 256 {
        return Err(invalid("capture selector exceeds 256 bytes"));
    }
    let identifier = std::str::from_utf8(&bytes)
        .map_err(|_| invalid("invalid capture identifier"))?
        .trim();
    if !valid_identifier(identifier) {
        return Err(invalid("invalid capture identifier"));
    }
    reject_retired(&root, identifier)?;
    let metadata_path = recovery::absolute(
        &root
            .join("recovery/captures")
            .join(format!("{identifier}.json")),
    )?;
    let snapshots = recovery::absolute(&root.join("recovery/snapshots"))?;
    if !std::fs::metadata(&snapshots)?.is_dir() {
        return Err(invalid("capture snapshots namespace is not a directory"));
    }
    let snapshot_path = recovery::absolute(&snapshots.join(format!("{identifier}.json")))?;
    // Decode and hash the same opened immutable carrier, rather than reopening it.
    let metadata_bytes = bounded_carrier(&metadata_path, 16 << 20)?;
    let pin_bytes = bounded_carrier(
        &root
            .join("recovery/pins")
            .join(format!("{identifier}.json")),
        16 << 20,
    )?;
    if pin_bytes != metadata_bytes {
        return Err(invalid(
            "capture metadata differs from its immutable retention pin",
        ));
    }
    let meta = super::codec::decode(&metadata_bytes)?;
    let backup_id = string(&meta, "backup_id")?;
    if !valid_identifier(backup_id) {
        return Err(invalid("invalid completed backup identifier"));
    }
    let directory = recovery::absolute(&root.join("base").join(backup_id))?;
    let manifest = recovery::absolute(&directory.join("backup_manifest"))?;
    let (manifest_value, manifest_digest) = super::backup_manifest::read(&manifest)?;
    let ranges = manifest_value["WAL-Ranges"]
        .as_array()
        .filter(|ranges| !ranges.is_empty())
        .ok_or_else(|| invalid("source-local capture requires manifest WAL ranges"))?;
    let timeline = meta["timeline"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= u32::MAX.into())
        .ok_or_else(|| invalid("invalid capture timeline"))?;
    let mut stop = 0;
    for range in ranges {
        let start = recovery::lsn(&range["Start-LSN"])?;
        let end = recovery::lsn(&range["End-LSN"])?;
        if range["Timeline"].as_u64() != Some(timeline) || start > end {
            return Err(invalid(
                "capture requires one consistent manifest WAL timeline and valid ranges",
            ));
        }
        stop = stop.max(end);
    }
    if recovery::lsn(&meta["backup_stop_lsn"])? != stop
        || recovery::lsn(&meta["post_backup_lsn"])? <= stop
    {
        return Err(invalid(
            "capture recovery point does not follow the actual manifest stop LSN",
        ));
    }
    let names = settings["record_checks"]
        .as_array()
        .ok_or_else(|| invalid("invalid record checks"))?
        .iter()
        .map(|v| string(v, "name"))
        .collect::<Result<BTreeSet<_>>>()?;
    let hashes = meta["record_hashes"]
        .as_object()
        .ok_or_else(|| invalid("invalid capture record hashes"))?;
    let segment = meta["wal_segment_bytes"]
        .as_u64()
        .filter(|n| (1 << 20..=1 << 30).contains(n) && n.is_power_of_two());
    if meta["version"].as_u64() != Some(1)
        || meta["capture_id"] != identifier
        || meta["manifest_sha256"] != manifest_digest
        || meta["record_contract_sha256"] != recovery::contract(settings)?
        || !meta["writer_fence_token"]
            .as_str()
            .is_some_and(writer_fence::valid_token)
        || meta["epoch_id"] != meta["writer_fence_token"]
        || segment.is_none()
        || names.is_empty()
        || hashes.keys().map(String::as_str).collect::<BTreeSet<_>>() != names
        || hashes.values().any(|v| {
            !v.as_str().is_some_and(|s| {
                s.len() == 64
                    && s.bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
        })
    {
        return Err(invalid(
            "invalid source-local capture metadata or manifest binding",
        ));
    }
    if writer_fence::major(&json!({"major":meta["pg_major"]}))? != writer_fence::major(config)?
        || meta["system_identifier"] != settings["system_identifier"]
    {
        return Err(invalid(
            "backup identity does not match the declared primary",
        ));
    }
    if recovery::lsn(&meta["post_backup_lsn"])? <= recovery::lsn(&meta["backup_stop_lsn"])? {
        return Err(invalid("backup recovery point must follow its stop LSN"));
    }
    let wal_bytes = bounded_carrier(
        &root
            .join("recovery/captures")
            .join(format!("{identifier}.wal-inventory")),
        16 << 20,
    )?;
    if codec::decode(&wal_bytes)?
        != super::recovery_capture::retained_wal(&root.join("wal"), &manifest_value, &meta)?
    {
        return Err(invalid(
            "retained WAL bytes differ from the immutable capture inventory",
        ));
    }
    let actual_now: i64 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| invalid("invalid system clock"))?
        .as_secs()
        .try_into()
        .map_err(|_| invalid("invalid system clock"))?;
    recovery::fresh(
        &meta["completed_at"],
        actual_now,
        settings["max_age_seconds"]
            .as_i64()
            .ok_or_else(|| invalid("invalid evidence age"))?,
    )?;
    use std::os::unix::fs::MetadataExt;
    recovery::fresh(
        &json!(std::fs::metadata(&manifest)?.mtime()),
        now,
        settings["max_age_seconds"]
            .as_i64()
            .ok_or_else(|| invalid("invalid evidence age"))?,
    )?;
    let binding = json!({"backup_id":backup_id,"system_identifier":settings["system_identifier"],"major":writer_fence::major(config)?,"epoch_id":meta["epoch_id"],"manifest_sha256":manifest_digest,"recovery_target_lsn":meta["post_backup_lsn"],"metadata_sha256":codec::digest(&metadata_bytes),"wal_inventory_sha256":codec::digest(&wal_bytes)});
    Ok(SelectedCapture {
        directory,
        metadata_path,
        snapshot_path,
        metadata: meta,
        binding,
    })
}
