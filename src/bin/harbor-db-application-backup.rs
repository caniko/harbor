use clap::{Parser, Subcommand};
use harbor_db::storage::{Result, application_backup, codec, durable};
use std::path::PathBuf;
#[derive(Parser)]
struct Args {
    #[arg(long)]
    config: PathBuf,
    #[command(subcommand)]
    command: Action,
}
#[derive(Subcommand)]
enum Action {
    Prune {
        #[arg(long)]
        keep: u32,
        #[arg(long)]
        incomplete_age_seconds: u64,
    },
    Capture {
        #[arg(long, conflicts_with = "scheduled")]
        attempt: Option<String>,
        #[arg(long, conflicts_with = "retry_incomplete")]
        scheduled: bool,
        #[arg(long)]
        retry_incomplete: bool,
    },
    Inspect {
        backup: PathBuf,
    },
    Certify {
        backup: PathBuf,
        #[arg(long)]
        state: PathBuf,
    },
}
fn run(a: Args) -> Result<()> {
    let c = durable::read_config_json(&a.config)?;
    let result = match a.command {
        Action::Prune {
            keep,
            incomplete_age_seconds,
        } => application_backup::prune(&c, keep, incomplete_age_seconds)?,
        Action::Capture {
            attempt,
            scheduled,
            retry_incomplete,
        } => {
            let attempt = attempt.unwrap_or_else(|| {
                format!(
                    "{}-{}-{}",
                    if scheduled { "timer" } else { "manual" },
                    chrono::Utc::now().format("%Y%m%dT%H%M%SZ"),
                    std::process::id()
                )
            });
            application_backup::capture(&c, &attempt, retry_incomplete)?
        }
        Action::Inspect { backup } => application_backup::inspect(&c, &backup, None)?,
        Action::Certify { backup, state } => application_backup::certify(&c, &backup, &state)?,
    };
    println!(
        "{}",
        String::from_utf8_lossy(&codec::encode(&result, false)?)
    );
    Ok(())
}
fn main() {
    if let Err(e) = run(Args::parse()) {
        eprintln!("harbor-db-application-backup: {e}");
        std::process::exit(1);
    }
}
