//! Controlled numerical probe using production physical-card reservations.
//! Invoke inside a bounded systemd user service; KFD isolation is not qualified.
use clap::Parser;
use harbor_cad::{
    lifecycle::NativeProcess,
    resources::{card_reservation_root, try_reserve_cards},
};
use std::{
    path::PathBuf,
    process::Command,
    thread,
    time::{Duration, Instant},
};

#[derive(Parser)]
struct Arguments {
    #[arg(long)]
    python: PathBuf,
    #[arg(long)]
    verifier: PathBuf,
    #[arg(long)]
    executable: PathBuf,
    #[arg(long)]
    cpu_executable: PathBuf,
    #[arg(long)]
    pci: String,
    #[arg(long)]
    uuid: String,
    #[arg(long)]
    output: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Arguments::parse();
    let root = card_reservation_root()?;
    let deadline = Instant::now() + Duration::from_secs(20);
    let _reservation = loop {
        if let Some(held) = try_reserve_cards(&root, std::slice::from_ref(&args.pci))? {
            break held;
        }
        if Instant::now() >= deadline {
            return Err("physical card reservation timed out before numerical probe".into());
        }
        thread::sleep(Duration::from_millis(100));
    };
    let mut command = Command::new(args.python.canonicalize()?);
    command.env_clear().arg(args.verifier.canonicalize()?);
    command
        .arg("--executable")
        .arg(args.executable.canonicalize()?)
        .arg("--cpu-executable")
        .arg(args.cpu_executable.canonicalize()?)
        .args(["--pci", &args.pci, "--uuid", &args.uuid, "--output"])
        .arg(args.output);
    let status = NativeProcess::spawn(command)?.wait(Duration::from_secs(880), || Ok(()))?;
    if !status.success() {
        return Err(format!("numerical probe failed: {status}").into());
    }
    Ok(())
}
