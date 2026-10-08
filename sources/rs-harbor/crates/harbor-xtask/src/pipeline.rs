use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

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

/// Run steps in order, failing fast on the first non-zero exit.
///
/// A failing captured step surfaces its stdout/stderr in the error message.
pub fn run_pipeline(steps: &[CommandSpec]) -> Result<Vec<CommandResult>> {
    let mut results = Vec::with_capacity(steps.len());
    for spec in steps {
        let result = run_step(spec)?;
        let ok = result.success;
        results.push(result);
        if !ok {
            let result = results.last().expect("just pushed");
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
            bail!(message);
        }
    }
    Ok(results)
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
