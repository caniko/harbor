//! `harbor-rs steam-runtime exec` — run a command inside a Steam Runtime
//! SDK container with the project working tree mounted as the current uid.
//!
//! Replaces the previous `steam-runtime-exec` shell wrapper. The Nix-side
//! `mkSteamRuntimeTools` bakes the default image into the writeShellApp
//! shim that calls into this subcommand, so callers don't need to know
//! the registry URL.

#![allow(clippy::needless_pass_by_value, clippy::map_unwrap_or)]

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};

#[derive(Subcommand, Debug)]
pub enum SteamRuntimeCommand {
    /// Run a command inside the configured Steam Runtime SDK container.
    Exec(SteamRuntimeExecArgs),
}

pub fn run(cmd: SteamRuntimeCommand) -> Result<()> {
    match cmd {
        SteamRuntimeCommand::Exec(args) => exec(args),
    }
}

#[derive(Args, Debug)]
pub struct SteamRuntimeExecArgs {
    /// Human-readable runtime label (printed for diagnostics).
    /// Repeats override earlier values, so the Nix shim can bake a
    /// default that callers may freely re-specify.
    #[arg(long, default_value = "sniper", overrides_with = "runtime")]
    pub runtime: String,

    /// Container image to execute in (registry path or local tag).
    /// Repeats override earlier values.
    #[arg(long, overrides_with = "image")]
    pub image: String,

    /// Container runner binary (`podman` or `docker`).
    /// Repeats override earlier values.
    #[arg(long, default_value = "podman", overrides_with = "container_runtime")]
    pub container_runtime: String,

    /// Allocate a TTY (`-it`).
    #[arg(long)]
    pub interactive: bool,

    /// Mount `/nix/store` read-only into the container so Nix-built tools
    /// referenced by absolute path resolve.
    #[arg(long)]
    pub mount_nix_store: bool,

    /// Working directory inside the container; defaults to the host CWD.
    #[arg(long)]
    pub workdir: Option<PathBuf>,

    /// Command and arguments to run inside the container.
    #[arg(trailing_var_arg = true, required = true)]
    pub command: Vec<String>,
}

pub fn exec(args: SteamRuntimeExecArgs) -> Result<()> {
    let runner = which::which(&args.container_runtime)
        .with_context(|| format!("container runtime not found: {}", args.container_runtime))?;

    let cwd = std::env::current_dir().context("reading current working directory")?;
    let workdir = args.workdir.clone().unwrap_or_else(|| cwd.clone());

    let mut cmd = Command::new(&runner);
    cmd.arg("run").arg("--rm");
    if args.interactive {
        cmd.arg("-it");
    }

    let runner_basename = runner.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if runner_basename == "podman" {
        cmd.arg("--userns=keep-id");
    }

    if args.mount_nix_store {
        let nix_store = std::path::Path::new("/nix/store");
        if !nix_store.is_dir() {
            bail!("--mount-nix-store requested but /nix/store does not exist");
        }
        cmd.arg("--volume").arg("/nix/store:/nix/store:ro");
    }

    cmd.arg("--volume")
        .arg(format!("{}:{}", cwd.display(), cwd.display()));
    cmd.arg("--workdir").arg(&workdir);

    let (uid, gid) = current_uid_gid()?;
    cmd.arg("--user").arg(format!("{uid}:{gid}"));

    cmd.arg(&args.image);
    cmd.args(&args.command);

    eprintln!(
        "harbor-rs steam-runtime exec: runtime={} image={} runner={}",
        args.runtime,
        args.image,
        runner.display(),
    );

    let status = cmd
        .status()
        .with_context(|| format!("spawning {}", runner.display()))?;
    if !status.success() {
        bail!(
            "{} run exited with {}",
            runner.display(),
            status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "<signal>".to_string()),
        );
    }
    Ok(())
}

/// Resolve the current uid/gid by shelling out to `id` (matching the
/// behaviour of the previous shell wrapper). Cheap enough that a fork
/// per invocation isn't worth a `libc` dependency or an unsafe block.
fn current_uid_gid() -> Result<(u32, u32)> {
    let uid = read_id("-u")?;
    let gid = read_id("-g")?;
    Ok((uid, gid))
}

fn read_id(flag: &str) -> Result<u32> {
    let output = Command::new("id")
        .arg(flag)
        .output()
        .with_context(|| format!("running id {flag}"))?;
    if !output.status.success() {
        bail!("id {flag} exited with {:?}", output.status.code());
    }
    let text = String::from_utf8(output.stdout)
        .with_context(|| format!("id {flag} produced non-utf8 output"))?;
    text.trim()
        .parse::<u32>()
        .with_context(|| format!("id {flag} output `{}` is not a u32", text.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser, Debug)]
    struct Test {
        #[command(flatten)]
        args: SteamRuntimeExecArgs,
    }

    #[test]
    fn parses_required_image_and_command() {
        let cli = Test::try_parse_from([
            "t",
            "--image",
            "registry.example.com/sniper/sdk",
            "--",
            "bash",
            "-lc",
            "echo hi",
        ])
        .unwrap();
        assert_eq!(cli.args.image, "registry.example.com/sniper/sdk");
        assert_eq!(cli.args.runtime, "sniper");
        assert_eq!(cli.args.container_runtime, "podman");
        assert!(!cli.args.interactive);
        assert!(!cli.args.mount_nix_store);
        assert_eq!(cli.args.command, vec!["bash", "-lc", "echo hi"]);
    }

    #[test]
    fn parses_overrides() {
        let cli = Test::try_parse_from([
            "t",
            "--runtime",
            "scout",
            "--image",
            "img",
            "--container-runtime",
            "docker",
            "--interactive",
            "--mount-nix-store",
            "--",
            "true",
        ])
        .unwrap();
        assert_eq!(cli.args.runtime, "scout");
        assert_eq!(cli.args.container_runtime, "docker");
        assert!(cli.args.interactive);
        assert!(cli.args.mount_nix_store);
    }

    #[test]
    fn rejects_missing_command() {
        assert!(Test::try_parse_from(["t", "--image", "img"]).is_err());
    }

    /// The Nix shim bakes a default `--image`/`--container-runtime`/`--runtime`
    /// before forwarding the caller's args. Callers must be able to re-specify
    /// any of those flags without clap erroring on duplicates.
    #[test]
    fn allows_baked_defaults_to_be_overridden() {
        let cli = Test::try_parse_from([
            "t",
            "--runtime",
            "sniper",
            "--image",
            "baked-image",
            "--container-runtime",
            "podman",
            "--container-runtime",
            "docker",
            "--image",
            "override-image",
            "--",
            "true",
        ])
        .unwrap();
        assert_eq!(cli.args.image, "override-image");
        assert_eq!(cli.args.container_runtime, "docker");
    }
}
