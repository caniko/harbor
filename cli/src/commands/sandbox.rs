use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use harbor_sandbox::{Desktop, Profile, RunOptions, Session};
use std::path::PathBuf;
use std::sync::{Arc, atomic::AtomicBool};

#[derive(Debug, Subcommand)]
pub enum SandboxCommand {
    /// Start a single isolated preview; return its exit status.
    Run(SessionArgs),
    /// Debounce edits, rebuild and replace only ready previews.
    Watch(SessionArgs),
    /// Inspect a session receipt and its kernel-backed supervisor lock.
    Status {
        #[arg(long)]
        session: PathBuf,
    },
    /// Cooperatively stop only this session's supervisor and namespaces.
    Stop {
        #[arg(long)]
        session: PathBuf,
    },
}

#[derive(Debug, Args)]
pub struct SessionArgs {
    #[arg(long)]
    profile: PathBuf,
    #[arg(long)]
    checkout: PathBuf,
    /// Private, mode-0700 directory outside the checkout. Evidence is retained.
    #[arg(long)]
    state_root: PathBuf,
    #[arg(long, default_value = "default")]
    variant: String,
}

pub fn run(command: SandboxCommand) -> Result<()> {
    let (args, watch) = match command {
        SandboxCommand::Status { session } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&harbor_sandbox::status(&session)?)?
            );
            return Ok(());
        }
        SandboxCommand::Stop { session } => return harbor_sandbox::stop(&session),
        SandboxCommand::Run(args) => (args, false),
        SandboxCommand::Watch(args) => (args, true),
    };
    let profile: Profile = serde_json::from_slice(&std::fs::read(args.profile)?)?;
    let parent_wayland = if profile.desktop == Desktop::Nested {
        let display = std::env::var_os("WAYLAND_DISPLAY")
            .context("nested profile requires WAYLAND_DISPLAY")?;
        let display = PathBuf::from(display);
        Some(if display.is_absolute() {
            display
        } else {
            PathBuf::from(
                std::env::var_os("XDG_RUNTIME_DIR")
                    .context("nested profile requires XDG_RUNTIME_DIR")?,
            )
            .join(display)
        })
    } else {
        None
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let sigint = signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&cancel))?;
    let sigterm = signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&cancel))?;
    let session = Session::open(&profile, &args.checkout, &args.state_root, &args.variant)?;
    eprintln!(
        "harbor-rs sandbox session: {}",
        session.directory().display()
    );
    let result = harbor_sandbox::run(
        session,
        RunOptions {
            watch,
            parent_wayland,
            cancel,
        },
    );
    signal_hook::low_level::unregister(sigint);
    signal_hook::low_level::unregister(sigterm);
    let exit = result?;
    if exit != 0 {
        std::process::exit(exit);
    }
    Ok(())
}
