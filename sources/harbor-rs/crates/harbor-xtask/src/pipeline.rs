use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use serde::Serialize;

/// A single command to run as part of a [`run_pipeline`] step.
///
/// The command inherits the caller's environment, then applies `env_add` and
/// `env_remove`; removals win, matching `std::process::Command::env_remove`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub label: String,
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env_add: Vec<(String, String)>,
    pub env_remove: Vec<String>,
    /// When true, stdout/stderr are captured into `CommandResult` instead of
    /// inherited.
    pub capture: bool,
}

impl CommandSpec {
    #[must_use]
    pub fn new(label: impl Into<String>, program: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            env_add: Vec::new(),
            env_remove: Vec::new(),
            capture: false,
        }
    }

    #[must_use]
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    #[must_use]
    pub fn args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    #[must_use]
    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    #[must_use]
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_add.push((key.into(), value.into()));
        self
    }

    #[must_use]
    pub fn env_remove(mut self, key: impl Into<String>) -> Self {
        self.env_remove.push(key.into());
        self
    }

    #[must_use]
    pub fn capture(mut self) -> Self {
        self.capture = true;
        self
    }
}

/// An ordered set of commands with optional best-effort cleanup steps.
///
/// Cleanup runs in reverse registration order even when a main step fails.
/// `keep_going` is deliberately opt-in: normal CI remains fail-fast, while
/// local diagnostic runs can collect more than one failure.
#[derive(Debug, Clone, Default)]
pub struct PipelinePlan {
    steps: Vec<CommandSpec>,
    cleanup: Vec<CommandSpec>,
    keep_going: bool,
}

impl PipelinePlan {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn step(mut self, spec: CommandSpec) -> Self {
        self.steps.push(spec);
        self
    }

    #[must_use]
    pub fn cleanup(mut self, spec: CommandSpec) -> Self {
        self.cleanup.push(spec);
        self
    }

    #[must_use]
    pub fn keep_going(mut self, enabled: bool) -> Self {
        self.keep_going = enabled;
        self
    }

    #[must_use]
    pub fn steps(&self) -> &[CommandSpec] {
        &self.steps
    }

    #[must_use]
    pub fn cleanup_steps(&self) -> &[CommandSpec] {
        &self.cleanup
    }

    /// Execute the plan and return a machine-readable report on success.
    pub fn run(&self) -> Result<PipelineReport> {
        let started = Instant::now();
        let results = execute_steps(&self.steps, &self.cleanup, self.keep_going)?;
        Ok(PipelineReport::from_results(results, started.elapsed()))
    }
}

/// Outcome of one [`CommandSpec`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandResult {
    pub label: String,
    pub success: bool,
    pub elapsed: Duration,
    /// Empty unless the step captured output.
    pub stdout: String,
    /// Empty unless the step captured output.
    pub stderr: String,
}

/// Serializable summary of a completed pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PipelineReport {
    pub success: bool,
    pub elapsed_ms: u128,
    pub steps: Vec<PipelineStepReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PipelineStepReport {
    pub label: String,
    pub success: bool,
    pub elapsed_ms: u128,
    pub stdout: String,
    pub stderr: String,
}

impl PipelineReport {
    fn from_results(results: Vec<CommandResult>, elapsed: Duration) -> Self {
        let success = results.iter().all(|result| result.success);
        Self {
            success,
            elapsed_ms: elapsed.as_millis(),
            steps: results
                .into_iter()
                .map(|result| PipelineStepReport {
                    label: result.label,
                    success: result.success,
                    elapsed_ms: result.elapsed.as_millis(),
                    stdout: result.stdout,
                    stderr: result.stderr,
                })
                .collect(),
        }
    }
}

pub fn write_report(path: impl AsRef<Path>, report: &PipelineReport) -> Result<()> {
    let path = path.as_ref();
    let bytes = serde_json::to_vec_pretty(report).context("serializing pipeline report")?;
    fs::write(path, bytes).with_context(|| format!("writing pipeline report {}", path.display()))
}

/// Run steps in order, failing fast on the first non-zero exit.
///
/// A failing captured step surfaces its stdout/stderr in the error message.
pub fn run_pipeline(steps: &[CommandSpec]) -> Result<Vec<CommandResult>> {
    execute_steps(steps, &[], false)
}

fn execute_steps(
    steps: &[CommandSpec],
    cleanup: &[CommandSpec],
    keep_going: bool,
) -> Result<Vec<CommandResult>> {
    let mut results = Vec::with_capacity(steps.len() + cleanup.len());
    let mut first_error = None;

    for spec in steps {
        match run_step(spec) {
            Ok(result) => {
                let failed = !result.success;
                if failed && first_error.is_none() {
                    first_error = Some(step_error(spec, &result));
                }
                results.push(result);
                if failed && !keep_going {
                    break;
                }
            }
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
                if !keep_going {
                    break;
                }
            }
        }
    }

    for spec in cleanup.iter().rev() {
        match run_step(spec) {
            Ok(result) => {
                if !result.success && first_error.is_none() {
                    first_error = Some(step_error(spec, &result));
                }
                results.push(result);
            }
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }

    if let Some(error) = first_error {
        Err(error)
    } else {
        Ok(results)
    }
}

fn step_error(spec: &CommandSpec, result: &CommandResult) -> anyhow::Error {
    let mut message = format!("{} failed after {:?}", result.label, result.elapsed);
    if spec.capture {
        if !result.stdout.is_empty() {
            message.push_str("\nstdout:\n");
            message.push_str(&result.stdout);
        }
        if !result.stderr.is_empty() {
            message.push_str("\nstderr:\n");
            message.push_str(&result.stderr);
        }
    }
    anyhow!(message)
}

/// Run one step without failing on its exit status; inspect
/// [`CommandResult::success`] yourself.
pub fn run_step(spec: &CommandSpec) -> Result<CommandResult> {
    let mut command = Command::new(&spec.program);
    command.args(&spec.args);
    if let Some(cwd) = &spec.cwd {
        command.current_dir(cwd);
    }
    for (key, value) in &spec.env_add {
        command.env(key, value);
    }
    for key in &spec.env_remove {
        command.env_remove(key);
    }

    let command_line = format!("{} {}", spec.program, spec.args.join(" "));
    tracing::debug!("running step {}: {command_line}", spec.label);

    let start = Instant::now();
    let (success, stdout, stderr) = if spec.capture {
        let output = command
            .output()
            .with_context(|| format!("running {command_line}"))?;
        (
            output.status.success(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    } else {
        let status = command
            .status()
            .with_context(|| format!("running {command_line}"))?;
        (status.success(), String::new(), String::new())
    };
    let elapsed = start.elapsed();

    Ok(CommandResult {
        label: spec.label.clone(),
        success,
        elapsed,
        stdout,
        stderr,
    })
}
