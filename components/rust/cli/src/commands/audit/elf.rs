//! `harbor-rs audit elf` — Steam-Runtime / Linux ELF dependency auditor.

#![allow(
    clippy::doc_markdown,
    clippy::needless_pass_by_value,
    clippy::manual_let_else,
    clippy::ref_binding_to_reference,
    clippy::if_same_then_else
)]

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use clap::Args;
use goblin::elf::dynamic::{DT_NEEDED, DT_RPATH, DT_RUNPATH};
use goblin::elf::{Elf, dynamic::Dynamic};
use regex::Regex;

use super::common::{Report, collect_files, compile_regex, require_match};

const DEFAULT_ALLOW_NEEDED_REGEX: &str = ".*";
const DEFAULT_FORBID_PATH_REGEX: &str = "(/nix/store|/usr/local|/home/)";

#[derive(Args, Debug)]
pub struct AuditElfArgs {
    /// Every ELF DT_NEEDED soname must match this regex.
    #[arg(long, default_value = DEFAULT_ALLOW_NEEDED_REGEX)]
    pub allow_needed_regex: String,

    /// RPATH/RUNPATH entries and ldd output must not match this regex.
    #[arg(long, default_value = DEFAULT_FORBID_PATH_REGEX)]
    pub forbid_path_regex: String,

    /// Require `$ORIGIN` in RPATH/RUNPATH (e.g. for redistributable Steam builds).
    #[arg(long)]
    pub require_origin_rpath: bool,

    /// Skip the runtime `ldd` resolution check.
    #[arg(long)]
    pub skip_ldd: bool,

    /// File or directory to audit.
    pub input: PathBuf,
}

pub fn run(args: AuditElfArgs) -> Result<()> {
    let allow_needed = compile_regex("allow-needed", &args.allow_needed_regex)?;
    let forbid_path = compile_regex("forbid-path", &args.forbid_path_regex)?;

    let mut report = Report::default();
    for path in collect_files(&args.input)? {
        check_file(&path, &args, &allow_needed, &forbid_path, &mut report)?;
    }
    report.finish("audit-elf-runtime-deps", &args.input)
}

fn check_file(
    path: &Path,
    args: &AuditElfArgs,
    allow_needed: &Regex,
    forbid_path: &Regex,
    report: &mut Report,
) -> Result<()> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return Ok(()),
    };
    let elf = match Elf::parse(&bytes) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };
    if !matches!(
        elf.header.e_type,
        goblin::elf::header::ET_EXEC | goblin::elf::header::ET_DYN
    ) {
        return Ok(());
    }

    report.note_checked();
    println!("audit-elf-runtime-deps: checking {}", path.display());

    let dynamic = elf.dynamic.as_ref();

    let runpaths = elf.runpaths.clone();
    let rpaths = elf.rpaths.clone();
    let combined_paths: Vec<&str> = rpaths.iter().chain(runpaths.iter()).copied().collect();

    if let Some(ref bad) = combined_paths
        .iter()
        .find(|entry| forbid_path.is_match(entry))
    {
        report.fail(format!(
            "forbidden path in RPATH/RUNPATH for {}: {}",
            path.display(),
            bad
        ));
    }

    if args.require_origin_rpath {
        let has_origin = combined_paths.iter().any(|entry| entry.contains("$ORIGIN"));
        if !has_origin {
            report.fail(format!(
                "missing $ORIGIN RPATH/RUNPATH in {}",
                path.display()
            ));
        }
    }

    let needed = needed_libraries(&elf, dynamic);
    for soname in &needed {
        require_match(
            report,
            allow_needed,
            soname,
            &format!("disallowed DT_NEEDED in {}: {soname}", path.display()),
        );
    }

    if !args.skip_ldd {
        check_ldd(path, forbid_path, report)?;
    }

    Ok(())
}

fn needed_libraries(elf: &Elf, dynamic: Option<&Dynamic>) -> Vec<String> {
    let Some(dyn_section) = dynamic else {
        return Vec::new();
    };
    dyn_section
        .dyns
        .iter()
        .filter_map(|d| {
            let tag = d.d_tag;
            let value = u32::try_from(d.d_val).ok()?;
            // RPATH/RUNPATH already exposed via elf.rpaths/runpaths above.
            if tag == DT_NEEDED {
                elf.dynstrtab.get_at(value as usize).map(str::to_string)
            } else if tag == DT_RPATH || tag == DT_RUNPATH {
                None
            } else {
                None
            }
        })
        .collect()
}

fn check_ldd(path: &Path, forbid_path: &Regex, report: &mut Report) -> Result<()> {
    let ldd = which::which("ldd").ok().or_else(|| {
        let candidate = std::path::Path::new("/usr/bin/ldd");
        candidate.is_file().then(|| candidate.to_path_buf())
    });
    let Some(ldd) = ldd else {
        return Ok(());
    };

    let output = Command::new(&ldd)
        .arg(path)
        .output()
        .with_context(|| format!("running {} {}", ldd.display(), path.display()))?;
    let combined = String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr);

    if combined.contains("not found") {
        report.fail(format!("missing library for {}", path.display()));
        eprintln!("{combined}");
    }
    for line in combined.lines() {
        if forbid_path.is_match(line) {
            report.fail(format!(
                "forbidden resolved path for {}: {line}",
                path.display()
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser, Debug)]
    struct Test {
        #[command(flatten)]
        args: AuditElfArgs,
    }

    #[test]
    fn parses_minimal_args() {
        let cli = Test::try_parse_from(["t", "dist/linux/foo"]).unwrap();
        assert_eq!(cli.args.input, PathBuf::from("dist/linux/foo"));
        assert_eq!(cli.args.allow_needed_regex, DEFAULT_ALLOW_NEEDED_REGEX);
        assert!(!cli.args.require_origin_rpath);
    }

    #[test]
    fn parses_overrides() {
        let cli = Test::try_parse_from([
            "t",
            "--allow-needed-regex",
            "lib(c|m)",
            "--forbid-path-regex",
            "/junk",
            "--require-origin-rpath",
            "--skip-ldd",
            "dist/linux/",
        ])
        .unwrap();
        assert_eq!(cli.args.allow_needed_regex, "lib(c|m)");
        assert_eq!(cli.args.forbid_path_regex, "/junk");
        assert!(cli.args.require_origin_rpath);
        assert!(cli.args.skip_ldd);
    }
}
