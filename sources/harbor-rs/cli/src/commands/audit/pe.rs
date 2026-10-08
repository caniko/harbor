//! `harbor-rs audit pe` — Windows PE dependency auditor.

#![allow(
    clippy::needless_pass_by_value,
    clippy::unnecessary_wraps,
    clippy::manual_let_else
)]

use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::Args;
use goblin::pe::PE;
use regex::Regex;

use super::common::{Report, collect_files, compile_regex, require_match};

const DEFAULT_ALLOW_DLL_REGEX: &str = ".*";
const DEFAULT_FORBID_PATH_REGEX: &str = "(/nix/store|/usr/local|/home/)";

#[derive(Args, Debug)]
pub struct AuditPeArgs {
    /// Every imported DLL name must match this regex (case-insensitive).
    #[arg(long, default_value = DEFAULT_ALLOW_DLL_REGEX)]
    pub allow_dll_regex: String,

    /// Embedded path strings must not match this regex.
    #[arg(long, default_value = DEFAULT_FORBID_PATH_REGEX)]
    pub forbid_path_regex: String,

    /// File or directory to audit.
    pub input: PathBuf,
}

pub fn run(args: AuditPeArgs) -> Result<()> {
    let allow_dll = compile_regex("allow-dll", &format!("(?i){}", args.allow_dll_regex))?;
    let forbid_path = compile_regex("forbid-path", &args.forbid_path_regex)?;

    let mut report = Report::default();
    for path in collect_files(&args.input)? {
        check_file(&path, &allow_dll, &forbid_path, &mut report)?;
    }
    report.finish("audit-windows-runtime-deps", &args.input)
}

fn check_file(
    path: &Path,
    allow_dll: &Regex,
    forbid_path: &Regex,
    report: &mut Report,
) -> Result<()> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return Ok(()),
    };
    // Use goblin's PE::parse directly rather than the Object dispatcher;
    // some PE32+ files round-trip through Object as Coff, which would be
    // silently skipped.
    let pe = match PE::parse(&bytes) {
        Ok(pe) => pe,
        Err(_) => return Ok(()),
    };

    report.note_checked();
    println!("audit-windows-runtime-deps: checking {}", path.display());

    for import in &pe.libraries {
        require_match(
            report,
            allow_dll,
            import,
            &format!("disallowed DLL import in {}: {import}", path.display()),
        );
    }

    for window in extract_printable_strings(&bytes, 4) {
        if forbid_path.is_match(&window) {
            report.fail(format!(
                "forbidden path string in {}: {window}",
                path.display()
            ));
            break;
        }
    }

    Ok(())
}

/// Iterate ASCII-printable runs of length >= `min_len`, modelled on the
/// behaviour of `strings(1)`. Used to spot embedded `/nix/store/...` paths
/// in release binaries.
fn extract_printable_strings(bytes: &[u8], min_len: usize) -> impl Iterator<Item = String> + '_ {
    bytes
        .split(|b| !(0x20..0x7f).contains(b) && *b != b'\t')
        .filter(move |span| span.len() >= min_len)
        .map(|span| String::from_utf8_lossy(span).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser, Debug)]
    struct Test {
        #[command(flatten)]
        args: AuditPeArgs,
    }

    #[test]
    fn parses_minimal() {
        let cli = Test::try_parse_from(["t", "dist/windows/"]).unwrap();
        assert_eq!(cli.args.input, PathBuf::from("dist/windows/"));
    }

    #[test]
    fn extracts_printable_runs() {
        let bytes = b"\x00abc\x01defg\x00hij";
        let runs: Vec<_> = extract_printable_strings(bytes, 4).collect();
        // "abc" is too short; "defg" passes; "hij" is too short.
        assert_eq!(runs, vec!["defg".to_string()]);
    }
}
