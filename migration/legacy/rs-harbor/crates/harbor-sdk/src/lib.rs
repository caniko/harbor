use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use tempfile::TempDir;
use thiserror::Error;

/// The realized macOS SDK outputs returned by `realize-macos-sdk --env`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealizedSdk {
    pub store_path: PathBuf,
    pub sdk_root: PathBuf,
    pub recursive_hash: String,
    pub version: String,
}

/// Options for realizing and pushing a macOS SDK archive to Attic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishOpts {
    pub archive: PathBuf,
    pub version: String,
    pub attic_server: String,
    pub cache_name: String,
    pub token_file: PathBuf,
    /// Optional GC-root link to register for the realized store path on the
    /// publishing host so it survives `nix-collect-garbage`.
    pub gc_root: Option<PathBuf>,
}

#[derive(Debug, Error)]
enum EnvParseError {
    #[error("missing required realize-macos-sdk output key: {0}")]
    MissingKey(&'static str),
    #[error("malformed realize-macos-sdk output line: {0}")]
    MalformedLine(String),
    #[error("invalid UTF-8 from command output")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),
}

/// Realize a macOS SDK archive and validate the output.
///
/// # Errors
///
/// Returns an error if the `realize-macos-sdk` or `validate-macos-sdk`
/// binaries cannot be found, if either command fails, or if the realized
/// `--env` output is missing required keys.
pub fn realize_macos_sdk(archive: &Path, version: &str) -> Result<RealizedSdk> {
    let realize =
        which::which("realize-macos-sdk").with_context(|| "finding realize-macos-sdk on PATH")?;
    let output = Command::new(&realize)
        .arg("--env")
        .arg(archive)
        .arg(version)
        .output()
        .with_context(|| format!("running {}", realize.display()))?;
    ensure_success(&realize, &output)?;

    let realized = parse_realize_env_output(&String::from_utf8(output.stdout)?)?;

    let validate =
        which::which("validate-macos-sdk").with_context(|| "finding validate-macos-sdk on PATH")?;
    let validate_output = Command::new(&validate)
        .arg(&realized.sdk_root)
        .arg(&realized.version)
        .output()
        .with_context(|| format!("running {}", validate.display()))?;
    ensure_success(&validate, &validate_output)?;

    Ok(realized)
}

/// Realize a macOS SDK archive and publish the realized store path to Attic.
///
/// # Errors
///
/// Returns an error if SDK realization fails, if the Attic binary cannot be
/// found, if the token file cannot be read, if the temporary config cannot be
/// written, or if `attic push` exits unsuccessfully.
pub fn publish_macos_sdk(opts: PublishOpts) -> Result<RealizedSdk> {
    let PublishOpts {
        archive,
        version,
        attic_server,
        cache_name,
        token_file,
        gc_root,
    } = opts;

    let realized = realize_macos_sdk(&archive, &version)?;
    // Pin the realized path locally before the push so a concurrent GC cannot
    // evict it mid-upload, and so it stays put even if the push fails.
    if let Some(link) = &gc_root {
        create_gc_root(&realized.store_path, link)?;
    }
    let attic = which::which("attic").with_context(|| "finding attic on PATH")?;
    let config_dir = TempDir::new().context("creating temporary attic config directory")?;
    let attic_config_dir = config_dir.path().join("attic");
    fs::create_dir_all(&attic_config_dir)
        .with_context(|| format!("creating {}", attic_config_dir.display()))?;
    let server_alias = "harbor-sdk";

    let token = fs::read_to_string(&token_file)
        .with_context(|| format!("reading {}", token_file.display()))?
        .trim()
        .to_owned();
    let config_path = attic_config_dir.join("config.toml");
    fs::write(
        &config_path,
        render_attic_config(server_alias, &attic_server, &token),
    )
    .with_context(|| format!("writing {}", config_path.display()))?;

    let output = Command::new(&attic)
        .env("XDG_CONFIG_HOME", config_dir.path())
        .arg("push")
        .arg(format!("{server_alias}:{cache_name}"))
        .arg(&realized.store_path)
        .output()
        .with_context(|| format!("running {}", attic.display()))?;
    ensure_success(&attic, &output)?;

    Ok(realized)
}

/// Register an indirect Nix GC root at `link` pointing to `store_path` so the
/// realized SDK survives `nix-collect-garbage`.
///
/// Uses `nix-store --realise --add-root --indirect`, which registers the root
/// without requiring elevated privileges (the link itself may live anywhere
/// the caller can write). Parent directories of `link` are created if missing.
///
/// # Errors
///
/// Returns an error if `nix-store` cannot be found on `PATH`, if the parent of
/// `link` cannot be created, or if `nix-store` exits unsuccessfully.
pub fn create_gc_root(store_path: &Path, link: &Path) -> Result<()> {
    let nix_store = which::which("nix-store").with_context(|| "finding nix-store on PATH")?;
    if let Some(parent) = link.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating gc-root parent {}", parent.display()))?;
    }
    let output = Command::new(&nix_store)
        .arg("--realise")
        .arg(store_path)
        .arg("--add-root")
        .arg(link)
        .arg("--indirect")
        .output()
        .with_context(|| format!("running {}", nix_store.display()))?;
    ensure_success(&nix_store, &output)
}

fn parse_realize_env_output(output: &str) -> Result<RealizedSdk, EnvParseError> {
    let mut values = HashMap::<String, String>::new();
    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let Some((key, value)) = line.split_once('=') else {
            return Err(EnvParseError::MalformedLine(line.to_owned()));
        };
        values.insert(key.trim().to_owned(), strip_outer_quotes(value.trim()));
    }

    let store_path = values
        .remove("STORE_PATH")
        .ok_or(EnvParseError::MissingKey("STORE_PATH"))?;
    let sdk_root = values
        .remove("SDK_ROOT")
        .ok_or(EnvParseError::MissingKey("SDK_ROOT"))?;
    let recursive_hash = values
        .remove("RECURSIVE_HASH")
        .ok_or(EnvParseError::MissingKey("RECURSIVE_HASH"))?;
    let version = values
        .remove("SDK_VERSION")
        .ok_or(EnvParseError::MissingKey("SDK_VERSION"))?;

    Ok(RealizedSdk {
        store_path: PathBuf::from(store_path),
        sdk_root: PathBuf::from(sdk_root),
        recursive_hash,
        version,
    })
}

fn strip_outer_quotes(value: &str) -> String {
    let quoted = value.as_bytes();
    if quoted.len() >= 2
        && ((quoted[0] == b'"' && quoted[quoted.len() - 1] == b'"')
            || (quoted[0] == b'\'' && quoted[quoted.len() - 1] == b'\''))
    {
        value[1..value.len() - 1].to_owned()
    } else {
        value.to_owned()
    }
}

fn render_attic_config(server: &str, endpoint: &str, token: &str) -> String {
    format!("[servers.{server:?}]\nendpoint = {endpoint:?}\ntoken = {token:?}\n")
}

fn ensure_success(program: &Path, output: &std::process::Output) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    bail!(
        "{} failed with status {}: {}",
        program.display(),
        output.status,
        stderr.trim(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_realize_env_output() {
        let parsed = parse_realize_env_output(
            r#"
STORE_PATH=/nix/store/abc123-macos-sdk
SDK_ROOT="/nix/store/abc123-macos-sdk/MacOSX26.1.sdk"
RECURSIVE_HASH="sha256-deadbeef"
SDK_VERSION=26.1
"#,
        )
        .expect("parse env output");

        assert_eq!(
            parsed,
            RealizedSdk {
                store_path: PathBuf::from("/nix/store/abc123-macos-sdk"),
                sdk_root: PathBuf::from("/nix/store/abc123-macos-sdk/MacOSX26.1.sdk"),
                recursive_hash: String::from("sha256-deadbeef"),
                version: String::from("26.1"),
            }
        );
    }
}
