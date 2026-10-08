//! `harbor-rs audit macho` — macOS Mach-O dependency auditor.

#![allow(
    clippy::needless_pass_by_value,
    clippy::unnecessary_wraps,
    clippy::manual_let_else
)]

use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::Args;
use goblin::Object;
use goblin::mach::{Mach, MachO};
use regex::Regex;

use super::common::{Report, collect_files, compile_regex, require_match};

const DEFAULT_ALLOW_DYLIB_REGEX: &str = "^(@executable_path|@rpath|/usr/lib/|/System/Library/)";
const DEFAULT_FORBID_PATH_REGEX: &str = "(/nix/store|/usr/local|/home/)";

#[derive(Args, Debug)]
pub struct AuditMachoArgs {
    /// Every Mach-O dependency path must match this regex.
    #[arg(long, default_value = DEFAULT_ALLOW_DYLIB_REGEX)]
    pub allow_dylib_regex: String,

    /// Dependency paths must not match this regex.
    #[arg(long, default_value = DEFAULT_FORBID_PATH_REGEX)]
    pub forbid_path_regex: String,

    /// File or directory to audit.
    pub input: PathBuf,
}

pub fn run(args: AuditMachoArgs) -> Result<()> {
    let allow_dylib = compile_regex("allow-dylib", &args.allow_dylib_regex)?;
    let forbid_path = compile_regex("forbid-path", &args.forbid_path_regex)?;

    let mut report = Report::default();
    for path in collect_files(&args.input)? {
        check_file(&path, &allow_dylib, &forbid_path, &mut report)?;
    }
    report.finish("audit-darwin-runtime-deps", &args.input)
}

fn check_file(
    path: &Path,
    allow_dylib: &Regex,
    forbid_path: &Regex,
    report: &mut Report,
) -> Result<()> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return Ok(()),
    };
    let mach = match Object::parse(&bytes) {
        Ok(Object::Mach(m)) => m,
        _ => return Ok(()),
    };

    report.note_checked();
    println!("audit-darwin-runtime-deps: checking {}", path.display());

    match mach {
        Mach::Binary(macho) => audit_macho(&macho, path, allow_dylib, forbid_path, report),
        Mach::Fat(fat) => {
            for slice in fat.into_iter().filter_map(Result::ok) {
                if let goblin::mach::SingleArch::MachO(macho) = slice {
                    audit_macho(&macho, path, allow_dylib, forbid_path, report);
                }
            }
        }
    }

    Ok(())
}

fn audit_macho(
    macho: &MachO,
    path: &Path,
    allow_dylib: &Regex,
    forbid_path: &Regex,
    report: &mut Report,
) {
    for dep in &macho.libs {
        // goblin seeds `libs[0]` with the literal `"self"` placeholder for
        // every Mach-O — it is not a real dependency. Empty entries are also
        // skipped defensively.
        if dep.is_empty() || *dep == "self" {
            continue;
        }
        if forbid_path.is_match(dep) {
            report.fail(format!(
                "forbidden dependency path in {}: {dep}",
                path.display()
            ));
            continue;
        }
        require_match(
            report,
            allow_dylib,
            dep,
            &format!("disallowed dependency in {}: {dep}", path.display()),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser, Debug)]
    struct Test {
        #[command(flatten)]
        args: AuditMachoArgs,
    }

    #[test]
    fn parses_minimal() {
        let cli = Test::try_parse_from(["t", "dist/macos/"]).unwrap();
        assert_eq!(cli.args.input, PathBuf::from("dist/macos/"));
        assert_eq!(cli.args.allow_dylib_regex, DEFAULT_ALLOW_DYLIB_REGEX);
    }
}
