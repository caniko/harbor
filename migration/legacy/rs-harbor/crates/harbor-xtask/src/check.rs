use anyhow::Result;

use crate::{CommandSpec, FormatMode, ProjectConfig, cargo_workspace_args, run_pipeline};

pub fn run_check(cfg: &ProjectConfig) -> Result<()> {
    let steps = [
        fmt_spec(cfg, FormatMode::Check),
        lint_spec(cfg),
        test_spec(cfg, &[]),
        build_spec(cfg, false),
    ];
    run_pipeline(&steps).map(drop)
}

pub fn run_fmt(cfg: &ProjectConfig, mode: FormatMode) -> Result<()> {
    run_pipeline(std::slice::from_ref(&fmt_spec(cfg, mode))).map(drop)
}

pub fn run_lint(cfg: &ProjectConfig) -> Result<()> {
    run_pipeline(std::slice::from_ref(&lint_spec(cfg))).map(drop)
}

pub fn run_test(cfg: &ProjectConfig, extra_args: &[String]) -> Result<()> {
    run_pipeline(std::slice::from_ref(&test_spec(cfg, extra_args))).map(drop)
}

pub fn run_cargo_build(cfg: &ProjectConfig, release: bool) -> Result<()> {
    run_pipeline(std::slice::from_ref(&build_spec(cfg, release))).map(drop)
}

fn fmt_spec(cfg: &ProjectConfig, mode: FormatMode) -> CommandSpec {
    let mut spec = CommandSpec::new("cargo fmt", "cargo").cwd(&cfg.workspace_root);
    spec = spec.arg("fmt").arg("--all");
    if mode == FormatMode::Check {
        spec = spec.arg("--").arg("--check");
    }
    spec
}

fn lint_spec(cfg: &ProjectConfig) -> CommandSpec {
    CommandSpec::new("cargo clippy", "cargo")
        .cwd(&cfg.workspace_root)
        .arg("clippy")
        .args(cargo_workspace_args(cfg))
        .arg("--")
        .arg("-D")
        .arg("warnings")
}

fn test_spec(cfg: &ProjectConfig, extra_args: &[String]) -> CommandSpec {
    CommandSpec::new("cargo test", "cargo")
        .cwd(&cfg.workspace_root)
        .arg("test")
        .args(cargo_workspace_args(cfg))
        .args(extra_args.iter().cloned())
}

fn build_spec(cfg: &ProjectConfig, release: bool) -> CommandSpec {
    let mut spec = CommandSpec::new("cargo build", "cargo")
        .cwd(&cfg.workspace_root)
        .arg("build")
        .args(cargo_workspace_args(cfg));
    if release {
        spec = spec.arg("--release");
    }
    spec
}

pub fn run_cargo_package(cfg: &ProjectConfig, package: &str, extra_args: &[String]) -> Result<()> {
    let mut spec = CommandSpec::new(format!("cargo run -p {package}"), "cargo")
        .cwd(&cfg.workspace_root)
        .arg("run")
        .arg("-p")
        .arg(package);
    if !extra_args.is_empty() {
        spec = spec.arg("--").args(extra_args.iter().cloned());
    }
    run_pipeline(std::slice::from_ref(&spec)).map(drop)
}
