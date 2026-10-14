use std::{collections::BTreeMap, path::PathBuf, thread, time::Duration};

use anyhow::{Result, ensure};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde_json::json;

use crate::{
    Manifest, accept, read_json, snapshot, validate_report, verify_http, verify_local,
    write_json_atomic,
};

#[derive(Parser)]
#[command(
    name = "harbor",
    version,
    about = "Shared Harbor development and production artifact contracts"
)]
pub struct Cli {
    #[command(subcommand)]
    command: Phase,
}

#[derive(Subcommand)]
enum Phase {
    /// Prepare immutable artifact identities and bind acceptance reports
    Dev {
        #[command(subcommand)]
        command: Dev,
    },
    /// Verify accepted local and served artifact bytes
    Prod {
        #[command(subcommand)]
        command: Prod,
    },
}

#[derive(Subcommand)]
enum Dev {
    Artifact {
        #[command(subcommand)]
        command: DevArtifact,
    },
}

#[derive(Subcommand)]
enum Prod {
    Artifact {
        #[command(subcommand)]
        command: ProdArtifact,
    },
}

#[derive(Subcommand)]
enum DevArtifact {
    Snapshot {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        roots: PathBuf,
    },
    Accept {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        report: PathBuf,
        #[arg(long)]
        required: PathBuf,
    },
}

#[derive(Subcommand)]
enum ProdArtifact {
    VerifyLocal {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        root: PathBuf,
        #[arg(long)]
        backend: Option<PathBuf>,
    },
    VerifyHttp {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        url: String,
        #[arg(long, default_value = "app/")]
        prefix: String,
        #[arg(long, default_value_t = 1)]
        attempts: u32,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Action {
    Snapshot,
    Accept,
    VerifyLocal,
    VerifyHttp,
}

/// Existing flags remain compatible with harbor-verify-artifact callers.
#[derive(Parser)]
#[command(name = "harbor-verify-artifact", version)]
pub struct CompatibilityCli {
    #[command(flatten)]
    args: CompatibilityArgs,
}

#[derive(Args)]
struct CompatibilityArgs {
    #[arg(value_enum)]
    action: Action,
    #[arg(long)]
    manifest: PathBuf,
    #[arg(long)]
    roots: Option<PathBuf>,
    #[arg(long)]
    report: Option<PathBuf>,
    #[arg(long)]
    required: Option<PathBuf>,
    #[arg(long)]
    root: Option<PathBuf>,
    #[arg(long)]
    backend: Option<PathBuf>,
    #[arg(long)]
    url: Option<String>,
    #[arg(long, default_value = "app/")]
    prefix: String,
    #[arg(long, default_value_t = 1)]
    attempts: u32,
}

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Phase::Dev {
            command: Dev::Artifact { command },
        } => match command {
            DevArtifact::Snapshot { manifest, roots } => prepare(&manifest, &roots),
            DevArtifact::Accept {
                manifest,
                report,
                required,
            } => bind(&manifest, &report, &required),
        },
        Phase::Prod {
            command: Prod::Artifact { command },
        } => match command {
            ProdArtifact::VerifyLocal {
                manifest,
                root,
                backend,
            } => local(&manifest, &root, backend.as_deref()),
            ProdArtifact::VerifyHttp {
                manifest,
                url,
                prefix,
                attempts,
            } => http(&manifest, &url, &prefix, attempts),
        },
    }
}

pub fn run_compatibility(cli: CompatibilityCli) -> Result<()> {
    let args = cli.args;
    match args.action {
        Action::Snapshot => prepare(
            &args.manifest,
            &args
                .roots
                .ok_or_else(|| anyhow::anyhow!("--roots is required"))?,
        ),
        Action::Accept => bind(
            &args.manifest,
            &args
                .report
                .ok_or_else(|| anyhow::anyhow!("--report is required"))?,
            &args
                .required
                .ok_or_else(|| anyhow::anyhow!("--required is required"))?,
        ),
        Action::VerifyLocal => local(
            &args.manifest,
            &args
                .root
                .ok_or_else(|| anyhow::anyhow!("--root is required"))?,
            args.backend.as_deref(),
        ),
        Action::VerifyHttp => http(
            &args.manifest,
            &args
                .url
                .ok_or_else(|| anyhow::anyhow!("--url is required"))?,
            &args.prefix,
            args.attempts,
        ),
    }
}

fn prepare(manifest: &std::path::Path, roots: &std::path::Path) -> Result<()> {
    let roots: BTreeMap<String, PathBuf> = read_json(roots)?;
    write_json_atomic(manifest, &snapshot(&roots)?, false)
}

fn bind(
    manifest: &std::path::Path,
    report: &std::path::Path,
    required: &std::path::Path,
) -> Result<()> {
    let accepted = accept(
        read_json(manifest)?,
        &read_json(report)?,
        &read_json::<Vec<String>>(required)?,
    )?;
    write_json_atomic(manifest, &accepted, true)
}

fn accepted(manifest: &std::path::Path) -> Result<Manifest> {
    let manifest: Manifest = read_json(manifest)?;
    let required = manifest
        .required_tests
        .as_ref()
        .filter(|tests| !tests.is_empty())
        .ok_or_else(|| anyhow::anyhow!("manifest has no required acceptance contract"))?;
    validate_report(&json!({"tests":manifest.tests}), required)?;
    Ok(manifest)
}

fn local(
    manifest: &std::path::Path,
    root: &std::path::Path,
    backend: Option<&std::path::Path>,
) -> Result<()> {
    verify_local(&accepted(manifest)?, root, backend)
}

fn http(manifest: &std::path::Path, url: &str, prefix: &str, attempts: u32) -> Result<()> {
    ensure!(attempts > 0, "verification attempts must be positive");
    let manifest = accepted(manifest)?;
    for attempt in 0..attempts {
        match verify_http(&manifest, url, prefix) {
            Ok(()) => return Ok(()),
            Err(error) if attempt + 1 == attempts => return Err(error),
            Err(_) => thread::sleep(Duration::from_secs(1)),
        }
    }
    unreachable!("positive attempts checked before loop")
}
