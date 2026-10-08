//! Linux development sessions with declared source inputs and isolated runtime state.
//! Profiles are trusted executable policy. Application processes see only store tools,
//! disposable source copies, their build cache and runtime state. No host HOME or bus
//! is inherited. Network, GPU and parent-display access are explicit capabilities.

mod process;
mod session;

pub use process::{Receipt, RunOptions, run, status, stop};
pub use session::{Session, Snapshot};

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Tools {
    pub bwrap: PathBuf,
    pub bash: PathBuf,
    pub dbus: Option<PathBuf>,
    pub sway: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Desktop {
    #[default]
    None,
    Headless,
    Nested,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Profile {
    pub schema_version: u32,
    pub name: String,
    pub tools: Tools,
    pub environment: BTreeMap<String, String>,
    pub shell_hook: String,
    /// Relative files/directories copied into each build generation. Symlinks and
    /// special files are rejected, including symlinks in the input's ancestors.
    pub inputs: Vec<PathBuf>,
    /// Selected input files copied to `HARBOR_ARTIFACTS/designs` for writable authoring.
    #[serde(default)]
    pub design_copies: Vec<PathBuf>,
    pub command: Vec<String>,
    /// Explicit mock/helper services owned by the candidate's private namespace.
    #[serde(default)]
    pub services: Vec<Vec<String>>,
    pub build: Option<Vec<String>>,
    /// Runs inside the candidate preview's namespaces before retiring the old preview.
    pub ready: Option<Vec<String>>,
    pub desktop: Desktop,
    pub network: bool,
    pub gpu: bool,
    pub cargo_config: Option<PathBuf>,
    pub poll_ms: u64,
    pub debounce_ms: u64,
    pub build_timeout_seconds: u64,
    pub ready_timeout_seconds: u64,
}

impl Profile {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1,
            "unsupported sandbox profile version"
        );
        validate_name(&self.name)?;
        ensure!(!self.inputs.is_empty(), "declare at least one source input");
        for input in &self.inputs {
            ensure!(
                relative(input),
                "input must be a normal relative path: {}",
                input.display()
            );
            ensure!(
                !matches!(input.components().next(), Some(Component::Normal(x)) if x == ".git" || x == "target" || x == ".direnv"),
                "build and Git state cannot be source inputs"
            );
        }
        for (i, input) in self.inputs.iter().enumerate() {
            ensure!(
                !self.inputs[..i]
                    .iter()
                    .any(|other| input.starts_with(other) || other.starts_with(input)),
                "overlapping source inputs"
            );
        }
        for copy in &self.design_copies {
            ensure!(
                relative(copy) && self.inputs.iter().any(|input| copy.starts_with(input)),
                "design copies must select declared relative inputs"
            );
        }
        for (key, value) in &self.environment {
            ensure!(
                valid_env(key) && !reserved(key),
                "invalid or sandbox-owned environment variable: {key}"
            );
            ensure!(!value.contains('\0'), "environment contains NUL");
        }
        for command in [
            &self.command,
            self.build.as_ref().unwrap_or(&self.command),
            self.ready.as_ref().unwrap_or(&self.command),
        ]
        .into_iter()
        .chain(&self.services)
        {
            ensure!(
                !command.is_empty() && !command[0].is_empty(),
                "empty sandbox command"
            );
            ensure!(
                !command.iter().any(|arg| arg.contains('\0')),
                "command contains NUL"
            );
        }
        for tool in [
            Some(&self.tools.bwrap),
            Some(&self.tools.bash),
            self.tools.dbus.as_ref(),
            self.tools.sway.as_ref(),
            self.cargo_config.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            ensure!(
                tool.is_absolute(),
                "tool/config path must be absolute: {}",
                tool.display()
            );
        }
        ensure!(
            self.poll_ms >= 20 && self.poll_ms <= 60_000 && self.debounce_ms <= 60_000,
            "invalid watch interval"
        );
        ensure!(
            (1..=3600).contains(&self.build_timeout_seconds)
                && (1..=300).contains(&self.ready_timeout_seconds),
            "invalid timeout"
        );
        if self.desktop != Desktop::None {
            ensure!(
                self.tools.dbus.is_some() && self.tools.sway.is_some(),
                "desktop profiles require dbus-run-session and sway"
            );
        }
        if let Some(config) = &self.cargo_config {
            ensure!(
                config.is_file(),
                "Cargo configuration must be a readable file: {}",
                config.display()
            );
        }
        Ok(())
    }
}

fn relative(path: &Path) -> bool {
    !path.as_os_str().is_empty() && path.components().all(|c| matches!(c, Component::Normal(_)))
}

fn validate_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name.len() <= 64
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "sandbox names must contain 1–64 ASCII letters, digits, '-' or '_'"
    );
    Ok(())
}

fn valid_env(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .enumerate()
            .all(|(i, b)| b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit()))
}

fn reserved(key: &str) -> bool {
    (key.starts_with("XDG_") && key != "XDG_DATA_DIRS")
        || key.starts_with("DBUS_")
        || key.starts_with("DIRENV_")
        || key.starts_with("SCCACHE_")
        || key.starts_with("AWS_")
        || matches!(
            key,
            "HOME"
                | "TMPDIR"
                | "TMP"
                | "TEMP"
                | "CARGO_HOME"
                | "CARGO_TARGET_DIR"
                | "RUSTUP_HOME"
                | "RUSTC_WRAPPER"
                | "DISPLAY"
                | "WAYLAND_DISPLAY"
                | "SWAYSOCK"
                | "AT_SPI_BUS_ADDRESS"
                | "SSH_AUTH_SOCK"
                | "BASH_ENV"
                | "ENV"
                | "LD_PRELOAD"
        )
}
