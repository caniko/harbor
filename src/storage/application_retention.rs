//! Serialized retention for timer-generated captures. Transition/manual names stay pinned.
use super::{Result, application_backup, application_publication, codec, durable, invalid, string};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

fn scheduled_entry(name: &str) -> Option<&str> {
    let (attempt, suffix) = name.split_once('.').unwrap_or((name, ""));
    let timestamp = attempt.strip_prefix("timer-")?;
    if !timestamp.is_ascii()
        || timestamp.len() < 18
        || timestamp.as_bytes()[16] != b'-'
        || !timestamp[17..].bytes().all(|b| b.is_ascii_digit())
        || chrono::NaiveDateTime::parse_from_str(&timestamp[..16], "%Y%m%dT%H%M%SZ").is_err()
    {
        return None;
    }
    if !matches!(
        suffix,
        "" | "partial" | "sealed" | "credentials" | "restore-workspace" | "intent.json" | "pruning"
    ) && !["abandoned-", "retry-credentials-"].iter().any(|prefix| {
        suffix
            .strip_prefix(prefix)
            .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
    }) {
        return None;
    }
    Some(attempt)
}

fn mounts() -> Result<Vec<PathBuf>> {
    fs::read_to_string("/proc/self/mountinfo")?
        .lines()
        .map(|line| {
            let encoded = line
                .split_whitespace()
                .nth(4)
                .ok_or_else(|| invalid("invalid mount inventory"))?
                .as_bytes();
            let mut decoded = Vec::new();
            let mut i = 0;
            while i < encoded.len() {
                if encoded[i] == b'\\' {
                    let digits = encoded
                        .get(i + 1..i + 4)
                        .filter(|s| s.iter().all(|b| (b'0'..=b'7').contains(b)))
                        .ok_or_else(|| invalid("invalid mount inventory escape"))?;
                    let value = (digits[0] - b'0') as u16 * 64
                        + (digits[1] - b'0') as u16 * 8
                        + (digits[2] - b'0') as u16;
                    decoded.push(
                        u8::try_from(value)
                            .map_err(|_| invalid("invalid mount inventory escape"))?,
                    );
                    i += 4;
                } else {
                    decoded.push(encoded[i]);
                    i += 1;
                }
            }
            use std::os::unix::ffi::OsStringExt;
            Ok(PathBuf::from(std::ffi::OsString::from_vec(decoded)))
        })
        .collect()
}

fn check_tree(path: &Path, device: u64, mounts: &[PathBuf]) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir() {
        if metadata.dev() != device || mounts.iter().any(|mount| mount == path) {
            return Err(invalid(
                "scheduled pruning cannot cross a mounted directory",
            ));
        }
        for entry in fs::read_dir(path)? {
            check_tree(&entry?.path(), device, mounts)?;
        }
    }
    // remove_dir_all and unlink never follow symlinks in abandoned worker output.
    Ok(())
}

pub fn prune(config: &Value, keep: u32, incomplete_age: u64) -> Result<Value> {
    let root = application_backup::validate(config, true)?;
    super::custody::validate_path(string(config, "root")?)?;
    if !(1..=4096).contains(&keep)
        || incomplete_age > 90 * 86400
        || incomplete_age < config["timeout_seconds"].as_u64().unwrap_or(86400) * 5 + 60
    {
        return Err(invalid(
            "scheduled retention requires bounded keep and an incomplete age beyond all capture stage budgets",
        ));
    }
    if config["version"] == 2 {
        application_publication::require_namespace(&root)?;
    }
    let _lease = durable::lock(&root.join("lock"), false, false)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| invalid("invalid retention clock"))?
        .as_secs();
    let mut groups: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for entry in fs::read_dir(&root)? {
        let entry = entry?;
        if let Some(attempt) = entry.file_name().to_str().and_then(scheduled_entry) {
            if entry.file_type()?.is_symlink() {
                return Err(invalid("scheduled retention entry is redirected"));
            }
            groups
                .entry(attempt.to_owned())
                .or_default()
                .push(entry.path());
        }
    }
    let latest = if root.join("LAST_SUCCESS").try_exists()? {
        let marker = durable::read_json(&root.join("LAST_SUCCESS"))?;
        let attempt = string(&marker, "attempt")?;
        if !super::resource::valid_name(attempt)
            || codec::file_digest(&root.join(attempt).join("acceptance.json"))?
                != string(&marker, "acceptance_sha256")?
        {
            return Err(invalid("scheduled retention cannot trust LAST_SUCCESS"));
        }
        Some(attempt.to_owned())
    } else {
        None
    };
    let mut successes = Vec::new();
    let mut remove = Vec::new();
    for (attempt, entries) in &groups {
        let destination = root.join(attempt);
        let tombstone = root.join(format!("{attempt}.pruning"));
        if entries.contains(&destination) {
            if entries.contains(&tombstone) {
                return Err(invalid(
                    "scheduled retention has conflicting live and retiring output",
                ));
            }
            let receipt = durable::read_json(&destination.join("acceptance.json"))?;
            let captured = receipt["captured_at"]
                .as_u64()
                .filter(|time| *time <= now)
                .ok_or_else(|| invalid("invalid retained capture timestamp"))?;
            if receipt["status"] != "verified"
                || receipt["attempt"] != *attempt
                || receipt["resource"] != config["resource"]
            {
                return Err(invalid(
                    "scheduled retention encountered unbound successful output",
                ));
            }
            successes.push((captured, attempt.clone()));
            // The completed point and its retry intent remain authoritative.
            // Worker scratch is obsolete even while LAST_SUCCESS retains it.
            let intent = root.join(format!("{attempt}.intent.json"));
            remove.extend(
                entries
                    .iter()
                    .filter(|path| **path != destination && **path != intent)
                    .cloned(),
            );
        } else {
            let mut newest = 0;
            for path in entries {
                let modified = fs::symlink_metadata(path)?
                    .modified()?
                    .duration_since(UNIX_EPOCH)
                    .map_err(|_| invalid("invalid incomplete timestamp"))?
                    .as_secs();
                newest = newest.max(modified);
            }
            if entries.contains(&tombstone)
                || now
                    .checked_sub(newest)
                    .is_some_and(|age| age >= incomplete_age)
            {
                remove.extend(entries.iter().cloned());
            }
        }
    }
    successes.sort();
    successes.reverse();
    for (_, attempt) in successes.iter().skip(keep as usize) {
        if latest.as_ref() != Some(attempt) {
            remove.extend(groups[attempt].iter().cloned());
        }
    }
    remove.sort();
    remove.dedup();
    let mounts = mounts()?;
    let device = fs::metadata(&root)?.dev();
    for path in &remove {
        check_tree(path, device, &mounts)?;
    }
    let mut ordinary = Vec::new();
    let mut tombstones = Vec::new();
    // Rename accepted points durably before touching their contents. A partial
    // deletion can then resume without trying to revalidate a removed receipt.
    for path in &remove {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid("invalid scheduled retention path"))?;
        if scheduled_entry(name) == Some(name) {
            let tombstone = root.join(format!("{name}.pruning"));
            fs::rename(path, &tombstone)?;
            durable::sync_directory(&root)?;
            tombstones.push(tombstone);
        } else if name.ends_with(".pruning") {
            tombstones.push(path.clone());
        } else {
            ordinary.push(path.clone());
        }
    }
    // Keep the retiring directory until every sibling is gone, so interruptions
    // also resume intent/credential/workspace cleanup without a new grace period.
    for path in ordinary.iter().chain(&tombstones) {
        if fs::symlink_metadata(path)?.is_dir() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
        durable::sync_directory(&root)?;
    }
    Ok(
        json!({"status":"pruned","removed":remove,"retained_scheduled":successes.len()-successes.iter().skip(keep as usize).filter(|(_, a)| latest.as_ref()!=Some(a)).count()}),
    )
}
