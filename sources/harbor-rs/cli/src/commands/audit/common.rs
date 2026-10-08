//! Shared helpers for the audit subcommands.

#![allow(clippy::explicit_into_iter_loop, clippy::match_same_arms)]

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use regex::Regex;
use walkdir::WalkDir;

/// Collect every regular file under `path`. If `path` is itself a regular
/// file, returns just that one.
///
/// Permission-denied entries below the root are skipped silently to match
/// the previous shell scripts (which used `find -type f`); only failures
/// at the root path itself bubble up.
pub fn collect_files(path: &Path) -> Result<Vec<PathBuf>> {
    if !path.exists() {
        bail!("missing path: {}", path.display());
    }
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    let mut out = Vec::new();
    for entry in WalkDir::new(path).follow_links(false).into_iter() {
        match entry {
            Ok(entry) if entry.file_type().is_file() => {
                out.push(entry.path().to_path_buf());
            }
            Ok(_) => {}
            Err(_) => {
                // Skip permission-denied / vanished entries silently.
            }
        }
    }
    Ok(out)
}

/// Ensure `value` matches `regex`; otherwise add a failure to `report`.
pub fn require_match(report: &mut Report, regex: &Regex, value: &str, message: &str) {
    if !regex.is_match(value) {
        report.fail(message);
    }
}

/// Aggregator for one audit run. Tracks how many candidate files were
/// inspected and accumulates failure messages. Returns a non-zero exit
/// status when any failure was recorded *or* when zero files were
/// inspected (matches the behaviour of the previous shell scripts).
#[derive(Debug, Default)]
pub struct Report {
    pub checked: usize,
    pub failures: Vec<String>,
}

impl Report {
    pub fn note_checked(&mut self) {
        self.checked += 1;
    }

    pub fn fail(&mut self, message: impl Into<String>) {
        self.failures.push(message.into());
    }

    pub fn finish(self, kind: &str, input: &Path) -> Result<()> {
        for line in &self.failures {
            eprintln!("{kind}: {line}");
        }
        if self.checked == 0 {
            bail!(
                "{kind}: no candidate binaries found under {}",
                input.display()
            );
        }
        if !self.failures.is_empty() {
            bail!("{kind}: {} failure(s)", self.failures.len());
        }
        println!("{kind}: checked {} file(s)", self.checked);
        Ok(())
    }
}

pub fn compile_regex(label: &str, pattern: &str) -> Result<Regex> {
    Regex::new(pattern).with_context(|| format!("compiling {label} regex `{pattern}`"))
}
