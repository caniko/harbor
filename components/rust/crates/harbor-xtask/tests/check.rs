use std::path::PathBuf;

use harbor_xtask::{
    CargoCiOptions, CargoWorkspace, CheckProfile, ProjectConfig, TestRunner, cargo_ci_plan,
};

fn config() -> ProjectConfig {
    ProjectConfig {
        workspace_root: PathBuf::from("/tmp/project"),
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

#[test]
fn default_plan_has_stable_cargo_order_and_arguments() {
    let plan = cargo_ci_plan(
        &config(),
        CheckProfile::Default,
        CargoCiOptions {
            locked: true,
            all_features: true,
            ..CargoCiOptions::default()
        },
    );
    let commands = plan.steps();
    assert_eq!(commands.len(), 4);
    assert_eq!(commands[0].args, ["fmt", "--all", "--", "--check"]);
    assert_eq!(
        commands[1].args,
        [
            "clippy",
            "--workspace",
            "--all-features",
            "--locked",
            "--all-targets",
            "--",
            "-D",
            "warnings"
        ]
    );
    assert_eq!(
        commands[2].args,
        ["test", "--workspace", "--all-features", "--locked"]
    );
    assert_eq!(
        commands[3].args,
        ["build", "--workspace", "--all-features", "--locked"]
    );
}

#[test]
fn full_plan_adds_configured_quality_steps() {
    let plan = cargo_ci_plan(
        &config(),
        CheckProfile::Full,
        CargoCiOptions {
            test_runner: TestRunner::Nextest,
            docs: true,
            audit: true,
            deny: true,
            package: true,
            ..CargoCiOptions::default()
        },
    );
    assert_eq!(plan.steps().len(), 8);
    assert_eq!(plan.steps()[2].args[0..2], ["nextest", "run"]);
    assert_eq!(plan.steps()[5].args, ["audit"]);
    assert_eq!(plan.steps()[6].args, ["deny", "check"]);
    assert_eq!(plan.steps()[7].args[0..2], ["package", "--workspace"]);
}

#[test]
fn nextest_args_are_added_to_the_test_stage() {
    let plan = harbor_xtask::cargo_ci_plan_with_nextest_args(
        &ProjectConfig {
            cargo_workspace: CargoWorkspace {
                packages: Vec::new(),
                excludes: vec!["external-tests".to_owned()],
                all_features: false,
            },
            ..config()
        },
        CheckProfile::Default,
        CargoCiOptions {
            test_runner: TestRunner::Nextest,
            ..CargoCiOptions::default()
        },
        &["--profile".to_owned(), "ci".to_owned()],
    );
    assert_eq!(
        plan.steps()[1].args,
        [
            "clippy",
            "--workspace",
            "--exclude",
            "external-tests",
            "--all-features",
            "--locked",
            "--all-targets",
            "--",
            "-D",
            "warnings"
        ]
    );
    assert_eq!(
        plan.steps()[2].args,
        [
            "nextest",
            "run",
            "--workspace",
            "--exclude",
            "external-tests",
            "--all-features",
            "--locked",
            "--profile",
            "ci"
        ]
    );
}
