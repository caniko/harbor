use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
mod check;
mod ci;
mod copr;
mod coverage;
mod docs;
mod nix;
mod pipeline;
mod release;

pub use check::{
    CargoCiOptions, CheckProfile, TestRunner, cargo_ci_plan, cargo_ci_plan_with_nextest_args,
};
pub use check::{run_cargo_build, run_cargo_package, run_check, run_fmt, run_lint, run_test};
pub use ci::{HarborCiConfig, load_harbor_ci_config};
pub use copr::{run_copr_srpm, run_copr_vendor, run_copr_vendor_check};
pub use coverage::run_coverage;
pub use docs::run_docs_serve;
pub use nix::{run_nix_build, run_nix_develop};
pub use pipeline::{
    CommandResult, CommandSpec, PipelinePlan, PipelineReport, run_pipeline, run_step, write_report,
};
pub use release::{rewrite_spec_version, run_release};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectConfig {
    pub workspace_root: PathBuf,
    pub cargo_workspace: CargoWorkspace,
    pub spec_file: Option<PathBuf>,
    pub copr: Option<CoprConfig>,
    pub docs: Vec<DocsSite>,
    pub nix_packages: Vec<NixPackage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoWorkspace {
    pub packages: Vec<String>,
    pub excludes: Vec<String>,
    pub all_features: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoprConfig {
    pub source_archive_url_template: String,
    pub srpm_dir: PathBuf,
    pub vendor_tarball: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocsSite {
    pub name: String,
    pub root: PathBuf,
    pub engine: DocsEngine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocsEngine {
    Zola,
    Mdbook,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NixPackage {
    pub name: String,
    pub flake_ref: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageMode {
    Summary,
    Html,
    Lcov,
    Ci { fail_under_lines: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseMode {
    DryRun,
    Execute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatMode {
    Check,
    Write,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NixBuildOptions {
    pub impure: bool,
}

impl ProjectConfig {
    #[must_use]
    pub fn from_workspace_root(workspace_root: impl Into<PathBuf>) -> Self {
        Self {
            workspace_root: workspace_root.into(),
            cargo_workspace: CargoWorkspace {
                packages: Vec::new(),
                excludes: Vec::new(),
                all_features: false,
            },
            spec_file: None,
            copr: None,
            docs: Vec::new(),
            nix_packages: Vec::new(),
        }
    }

    #[must_use]
    pub fn resolve(&self, path: impl AsRef<Path>) -> PathBuf {
        let path = path.as_ref();
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.workspace_root.join(path)
        }
    }

    pub fn docs_site(&self, name: &str) -> Result<&DocsSite> {
        self.docs
            .iter()
            .find(|site| site.name == name)
            .ok_or_else(|| anyhow!("unknown docs site: {name}"))
    }

    pub fn nix_package(&self, name: &str) -> Result<&NixPackage> {
        self.nix_packages
            .iter()
            .find(|pkg| pkg.name == name)
            .ok_or_else(|| anyhow!("unknown nix package: {name}"))
    }
}

impl DocsSite {
    #[must_use]
    pub fn zola(name: impl Into<String>, root: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            root: root.into(),
            engine: DocsEngine::Zola,
        }
    }

    #[must_use]
    pub fn mdbook(name: impl Into<String>, root: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            root: root.into(),
            engine: DocsEngine::Mdbook,
        }
    }
}

pub(crate) fn cargo_workspace_args(cfg: &ProjectConfig) -> Vec<String> {
    let mut args = Vec::new();
    if cfg.cargo_workspace.packages.is_empty() {
        args.push(String::from("--workspace"));
        for package in &cfg.cargo_workspace.excludes {
            args.push(String::from("--exclude"));
            args.push(package.clone());
        }
    } else {
        for package in &cfg.cargo_workspace.packages {
            args.push(String::from("-p"));
            args.push(package.clone());
        }
    }
    if cfg.cargo_workspace.all_features {
        args.push(String::from("--all-features"));
    }
    args
}

pub(crate) fn run_command(command: &mut Command) -> Result<()> {
    let program = command.get_program().to_owned();
    let args = command.get_args().map(os_to_string).collect::<Vec<_>>();
    let command_line = format_command(&program, &args);

    tracing::debug!("running command: {command_line}");

    let status = command
        .status()
        .with_context(|| format!("running {command_line}"))?;
    if status.success() {
        Ok(())
    } else {
        bail!("{command_line} failed with status {status}")
    }
}

pub(crate) fn write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))
}

pub(crate) fn read_to_string(path: &Path) -> Result<String> {
    fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))
}

pub(crate) fn ensure_tool(tool: &str) -> Result<PathBuf> {
    which::which(tool).with_context(|| format!("finding {tool} on PATH"))
}

fn format_command(program: &OsStr, args: &[String]) -> String {
    std::iter::once(os_to_string(program))
        .chain(args.iter().cloned())
        .collect::<Vec<_>>()
        .join(" ")
}

fn os_to_string(value: &OsStr) -> String {
    value.to_string_lossy().into_owned()
}
