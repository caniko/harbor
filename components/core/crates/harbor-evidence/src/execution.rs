//! Atomic source-bound step journals. Executors own process control; applications
//! own reconciliation of partial effects. A journal never retries or cleans up.

use std::{fs::{File, OpenOptions}, path::{Path, PathBuf}};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use crate::write_json_atomic;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundFile {
    pub path: PathBuf,
    pub sha256: String,
}

impl BoundFile {
    pub fn capture(path: &Path) -> Result<Self> {
        let mut reader = File::open(path)?;
        Ok(Self {
            path: path.to_path_buf(),
            sha256: crate::artifact::digest(&mut reader)?,
        })
    }

    pub fn verify(&self) -> Result<()> {
        ensure!(Self::capture(&self.path)? == *self, "bound input changed: {}", self.path.display());
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectPolicy {
    ReadOnly,
    MayMutate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    NotExecuted,
    Running,
    Passed,
    Failed,
    TimedOut,
    Cancelled,
    UnknownEffect,
    SourceDrift,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StepIdentity {
    pub label: String,
    pub executable: BoundFile,
    pub inputs: Vec<BoundFile>,
    pub effect_policy: EffectPolicy,
}

/// Maps directly from an executor's phase-aware result. Text from arguments,
/// credentials or process errors must not be copied into this normalized record.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Observation {
    pub status: Status,
    pub exit_code: Option<i32>,
    pub elapsed_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StepReceipt {
    pub schema_version: u32,
    pub identity: StepIdentity,
    pub status: Status,
    pub observation: Option<Observation>,
    pub stdout: Option<BoundFile>,
    pub stderr: Option<BoundFile>,
    pub requires_reconciliation: bool,
}

fn log(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)] {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).context("creating private step log")
}

/// Run the supplied executor exactly once, retaining byte-preserving logs and
/// every interrupted/failed journal. The output directory must be caller-owned.
/// The callback must return only after its owned process group is settled, or
/// explicitly return `UnknownEffect` when that cannot be established.
pub fn record_step(
    directory: &Path,
    identity: StepIdentity,
    execute: impl FnOnce(&File, &File) -> Observation,
) -> Result<StepReceipt> {
    ensure!(!identity.label.is_empty(), "step label is required");
    ensure!(!identity.inputs.is_empty(), "step requires source/input bindings");
    let receipt_path = directory.join("receipt.json");
    let mut receipt = StepReceipt {
        schema_version: 1,
        identity,
        status: Status::NotExecuted,
        observation: None,
        stdout: None,
        stderr: None,
        requires_reconciliation: false,
    };
    // A fresh no-clobber journal must exist before any executor is called.
    write_json_atomic(&receipt_path, &receipt, false)?;
    receipt.identity.executable.verify()?;
    for input in &receipt.identity.inputs {
        input.verify()?;
    }
    let stdout_path = directory.join("stdout");
    let stderr_path = directory.join("stderr");
    let stdout = log(&stdout_path)?;
    let stderr = log(&stderr_path)?;
    receipt.status = Status::Running;
    receipt.requires_reconciliation = receipt.identity.effect_policy == EffectPolicy::MayMutate;
    write_json_atomic(&receipt_path, &receipt, true)?;
    let observation = execute(&stdout, &stderr);
    ensure!(matches!(observation.status, Status::NotExecuted | Status::Passed | Status::Failed | Status::TimedOut | Status::Cancelled | Status::UnknownEffect), "invalid terminal execution observation");
    ensure!(observation.status != Status::Passed || observation.exit_code == Some(0), "passing execution must have a successful normal exit");
    ensure!(observation.status != Status::Failed || observation.exit_code != Some(0), "failed execution cannot have a successful normal exit");
    ensure!(observation.status != Status::NotExecuted || observation.exit_code.is_none(), "unexecuted step cannot have an exit code");
    receipt.status = observation.status;
    receipt.requires_reconciliation &= receipt.status != Status::NotExecuted;
    receipt.observation = Some(observation);
    // Sync before hashing; any sync/read/write error leaves the Running record
    // intact and fail-closed. No mutation is replayed to repair missing evidence.
    stdout.sync_all()?;
    stderr.sync_all()?;
    receipt.stdout = Some(BoundFile::capture(&stdout_path)?);
    receipt.stderr = Some(BoundFile::capture(&stderr_path)?);
    if receipt.identity.executable.verify().is_err()
        || receipt.identity.inputs.iter().any(|input| input.verify().is_err())
    {
        receipt.status = Status::SourceDrift;
    }
    write_json_atomic(&receipt_path, &receipt, true)?;
    Ok(receipt)
}
