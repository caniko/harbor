use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use semver::Version;

use crate::{ProjectConfig, ReleaseMode, read_to_string, run_command, write_bytes};

pub fn rewrite_spec_version(spec_file: &Path, version: &Version) -> Result<()> {
    let original = read_to_string(spec_file)?;
    let mut rewritten = String::with_capacity(original.len());
    let mut rewrites = 0usize;

    for chunk in split_inclusive_preserving_last(&original) {
        let (line, newline) = split_line_ending(chunk);
        if line.starts_with("Version:") {
            rewrites += 1;
            write!(&mut rewritten, "Version:        {version}")
                .expect("writing into String cannot fail");
        } else {
            rewritten.push_str(line);
        }
        rewritten.push_str(newline);
    }

    match rewrites {
        1 => write_bytes(spec_file, rewritten.as_bytes()),
        0 => bail!(
            "expected exactly one Version: line in {}",
            spec_file.display()
        ),
        _ => bail!(
            "expected exactly one Version: line in {}, found {}",
            spec_file.display(),
            rewrites
        ),
    }
}

pub fn run_release(cfg: &ProjectConfig, version: &Version, mode: ReleaseMode) -> Result<()> {
    if let Some(spec_file) = &cfg.spec_file {
        let resolved = cfg.resolve(spec_file);
        match mode {
            ReleaseMode::DryRun => {
                let temp_dir = tempfile::tempdir().context("creating dry-run temp dir")?;
                let temp_spec = temp_dir.path().join(
                    resolved
                        .file_name()
                        .ok_or_else(|| anyhow::anyhow!("spec file has no file name"))?,
                );
                std::fs::copy(&resolved, &temp_spec)
                    .with_context(|| format!("copying {}", resolved.display()))?;
                rewrite_spec_version(&temp_spec, version)?;
            }
            ReleaseMode::Execute => {
                rewrite_spec_version(&resolved, version)?;

                let display_name = spec_file.display().to_string();

                let mut add = Command::new("git");
                add.current_dir(&cfg.workspace_root)
                    .arg("add")
                    .arg(spec_file);
                run_command(&mut add)?;

                let mut commit = Command::new("git");
                commit
                    .current_dir(&cfg.workspace_root)
                    .arg("commit")
                    .arg("-m")
                    .arg(format!("release: bump {display_name} Version to {version}"));
                run_command(&mut commit)?;
            }
        }
    }

    let mut command = Command::new("cargo");
    command
        .current_dir(&cfg.workspace_root)
        .arg("release")
        .arg(version.to_string())
        .arg("--workspace");
    if matches!(mode, ReleaseMode::Execute) {
        command.arg("--execute");
    }
    command.arg("--no-confirm");
    run_command(&mut command)
}

fn split_inclusive_preserving_last(input: &str) -> Vec<&str> {
    if input.is_empty() {
        return Vec::new();
    }
    input.split_inclusive('\n').collect()
}

fn split_line_ending(chunk: &str) -> (&str, &str) {
    if let Some(stripped) = chunk.strip_suffix("\r\n") {
        (stripped, "\r\n")
    } else if let Some(stripped) = chunk.strip_suffix('\n') {
        (stripped, "\n")
    } else {
        (chunk, "")
    }
}
