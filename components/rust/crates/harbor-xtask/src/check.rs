use anyhow::Result;

use crate::{
    CommandSpec, FormatMode, PipelinePlan, ProjectConfig, cargo_workspace_args, run_pipeline,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckProfile {
    Fast,
    Default,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestRunner {
    Cargo,
    Nextest,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CargoCiOptions {
    pub locked: bool,
    pub all_features: bool,
    pub test_runner: TestRunner,
    pub docs: bool,
    pub audit: bool,
    pub deny: bool,
    pub package: bool,
}

impl Default for CargoCiOptions {
    fn default() -> Self {
        Self {
            locked: true,
            all_features: true,
            test_runner: TestRunner::Cargo,
            docs: false,
            audit: false,
            deny: false,
            package: false,
        }
    }
}

#[must_use]
pub fn cargo_ci_plan(
    cfg: &ProjectConfig,
    profile: CheckProfile,
    options: CargoCiOptions,
) -> PipelinePlan {
    cargo_ci_plan_with_nextest_args(cfg, profile, options, &[])
}

#[must_use]
pub fn cargo_ci_plan_with_nextest_args(
    cfg: &ProjectConfig,
    profile: CheckProfile,
    options: CargoCiOptions,
    nextest_args: &[String],
) -> PipelinePlan {
    let nextest_args = if options.test_runner == TestRunner::Nextest {
        nextest_args
    } else {
        &[]
    };
    let mut plan = PipelinePlan::new()
        .step(fmt_spec(cfg, FormatMode::Check))
        .step(lint_spec_with(cfg, options))
        .step(test_spec_with(cfg, nextest_args, options));

    if matches!(profile, CheckProfile::Default | CheckProfile::Full) {
        plan = plan.step(build_spec_with(cfg, false, options));
    }
    if profile == CheckProfile::Full {
        if options.docs {
            plan = plan.step(doc_spec(cfg, options));
        }
        if options.audit {
            plan = plan.step(audit_spec(cfg));
        }
        if options.deny {
            plan = plan.step(deny_spec(cfg));
        }
        if options.package {
            plan = plan.step(package_spec(cfg, options));
        }
    }
    plan
}

pub fn run_check(cfg: &ProjectConfig) -> Result<()> {
    cargo_ci_plan(
        cfg,
        CheckProfile::Default,
        CargoCiOptions {
            all_features: cfg.cargo_workspace.all_features,
            ..CargoCiOptions::default()
        },
    )
    .run()
    .map(drop)
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
    lint_spec_with(
        cfg,
        CargoCiOptions {
            all_features: cfg.cargo_workspace.all_features,
            ..CargoCiOptions::default()
        },
    )
}

fn lint_spec_with(cfg: &ProjectConfig, options: CargoCiOptions) -> CommandSpec {
    CommandSpec::new("cargo clippy", "cargo")
        .cwd(&cfg.workspace_root)
        .arg("clippy")
        .args(cargo_workspace_args_with(
            cfg,
            options.all_features,
            options.locked,
        ))
        .arg("--all-targets")
        .arg("--")
        .arg("-D")
        .arg("warnings")
}

fn test_spec(cfg: &ProjectConfig, extra_args: &[String]) -> CommandSpec {
    test_spec_with(
        cfg,
        extra_args,
        CargoCiOptions {
            all_features: cfg.cargo_workspace.all_features,
            ..CargoCiOptions::default()
        },
    )
}

fn test_spec_with(
    cfg: &ProjectConfig,
    extra_args: &[String],
    options: CargoCiOptions,
) -> CommandSpec {
    let (program, command) = match options.test_runner {
        TestRunner::Cargo => ("cargo", "test"),
        TestRunner::Nextest => ("cargo", "nextest"),
    };
    let mut spec = CommandSpec::new(format!("cargo {command}"), program)
        .cwd(&cfg.workspace_root)
        .arg(command);
    if options.test_runner == TestRunner::Nextest {
        spec = spec.arg("run");
    }
    spec.args(cargo_workspace_args_with(
        cfg,
        options.all_features,
        options.locked,
    ))
    .args(extra_args.iter().cloned())
}

fn build_spec(cfg: &ProjectConfig, release: bool) -> CommandSpec {
    build_spec_with(
        cfg,
        release,
        CargoCiOptions {
            all_features: cfg.cargo_workspace.all_features,
            ..CargoCiOptions::default()
        },
    )
}

fn build_spec_with(cfg: &ProjectConfig, release: bool, options: CargoCiOptions) -> CommandSpec {
    let mut spec = CommandSpec::new("cargo build", "cargo")
        .cwd(&cfg.workspace_root)
        .arg("build")
        .args(cargo_workspace_args_with(
            cfg,
            options.all_features,
            options.locked,
        ));
    if release {
        spec = spec.arg("--release");
    }
    spec
}

fn doc_spec(cfg: &ProjectConfig, options: CargoCiOptions) -> CommandSpec {
    CommandSpec::new("cargo doc", "cargo")
        .cwd(&cfg.workspace_root)
        .arg("doc")
        .args(cargo_workspace_args_with(
            cfg,
            options.all_features,
            options.locked,
        ))
        .arg("--no-deps")
}

fn audit_spec(cfg: &ProjectConfig) -> CommandSpec {
    CommandSpec::new("cargo audit", "cargo")
        .cwd(&cfg.workspace_root)
        .arg("audit")
}

fn deny_spec(cfg: &ProjectConfig) -> CommandSpec {
    CommandSpec::new("cargo deny", "cargo")
        .cwd(&cfg.workspace_root)
        .arg("deny")
        .arg("check")
}

fn package_spec(cfg: &ProjectConfig, options: CargoCiOptions) -> CommandSpec {
    let mut spec = CommandSpec::new("cargo package", "cargo")
        .cwd(&cfg.workspace_root)
        .arg("package")
        .args(cargo_workspace_args_with(cfg, false, options.locked))
        .arg("--allow-dirty")
        .arg("--list");
    if options.all_features {
        spec = spec.arg("--all-features");
    }
    spec
}

fn cargo_workspace_args_with(cfg: &ProjectConfig, all_features: bool, locked: bool) -> Vec<String> {
    let mut args = cargo_workspace_args(cfg);
    args.retain(|arg| arg != "--all-features" && arg != "--locked");
    if all_features {
        args.push(String::from("--all-features"));
    }
    if locked {
        args.push(String::from("--locked"));
    }
    args
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
