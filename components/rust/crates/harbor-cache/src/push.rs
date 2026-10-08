use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use anyhow::{Context, Result, bail};
use tempfile::TempDir;

/// Options for pushing a store path into Attic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PushOpts<'a> {
    pub cache: &'a str,
    pub store_path: &'a Path,
    pub server: Option<&'a str>,
    pub token_file: Option<&'a Path>,
}

/// Push a store path into Attic, optionally using a temporary config.
///
/// # Errors
///
/// Returns an error if `attic` cannot be spawned, if it exits unsuccessfully,
/// if the token file cannot be read, if the temporary config cannot be written,
/// or if exactly one of `server` and `token_file` is provided.
pub fn push(opts: PushOpts<'_>) -> Result<()> {
    match (opts.server, opts.token_file) {
        (None, None) => push_with_ambient_config(opts.cache, opts.store_path),
        (Some(server), Some(token_file)) => {
            push_with_temp_config(opts.cache, opts.store_path, server, token_file)
        }
        _ => bail!("server and token_file must be provided together or omitted together"),
    }
}

fn push_with_ambient_config(cache: &str, store_path: &Path) -> Result<()> {
    let output = Command::new("attic")
        .arg("push")
        .arg(cache)
        .arg(store_path)
        .output()
        .context("running attic push with ambient config")?;
    ensure_success("attic", &output)
}

fn push_with_temp_config(
    cache: &str,
    store_path: &Path,
    server: &str,
    token_file: &Path,
) -> Result<()> {
    let config_dir = TempDir::new().context("creating temporary attic config directory")?;
    let attic_config_dir = config_dir.path().join("attic");
    fs::create_dir_all(&attic_config_dir)
        .with_context(|| format!("creating {}", attic_config_dir.display()))?;

    let token = fs::read_to_string(token_file)
        .with_context(|| format!("reading {}", token_file.display()))?
        .trim()
        .to_owned();

    let config_path = attic_config_dir.join("config.toml");
    fs::write(&config_path, render_attic_config(server, server, &token))
        .with_context(|| format!("writing {}", config_path.display()))?;

    let output = Command::new("attic")
        .env("XDG_CONFIG_HOME", config_dir.path())
        .arg("push")
        .arg(format!("{server}:{cache}"))
        .arg(store_path)
        .output()
        .context("running attic push with temporary config")?;
    ensure_success("attic", &output)
}

fn render_attic_config(server: &str, endpoint: &str, token: &str) -> String {
    format!("[servers.{server:?}]\nendpoint = {endpoint:?}\ntoken = {token:?}\n")
}

fn ensure_success(program: &str, output: &Output) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    bail!("{program} exited with {}: {}", output.status, stderr.trim())
}

#[cfg(test)]
mod tests {
    use super::{PushOpts, push, render_attic_config};
    use std::path::Path;

    #[test]
    fn renders_attic_config() {
        assert_eq!(
            render_attic_config("canix", "canix", "secret"),
            "[servers.\"canix\"]\nendpoint = \"canix\"\ntoken = \"secret\"\n"
        );
    }

    #[test]
    fn rejects_half_configured_push() {
        let err = push(PushOpts {
            cache: "canix",
            store_path: Path::new("/nix/store/example"),
            server: Some("canix"),
            token_file: None,
        })
        .expect_err("half-configured push should fail");
        assert!(
            err.to_string()
                .contains("server and token_file must be provided together"),
            "{err}"
        );
    }
}
