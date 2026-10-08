use std::process::Command;

use anyhow::Result;

use crate::{NixBuildOptions, ProjectConfig, run_command};

pub fn run_nix_build(cfg: &ProjectConfig, package: &str, opts: NixBuildOptions) -> Result<()> {
    let package = cfg.nix_package(package)?;
    let mut command = Command::new("nix");
    command.current_dir(&cfg.workspace_root).arg("build");
    if opts.impure {
        command.arg("--impure");
    }
    command.arg(&package.flake_ref);
    run_command(&mut command)
}

pub fn run_nix_develop(cfg: &ProjectConfig) -> Result<()> {
    let mut command = Command::new("nix");
    command.current_dir(&cfg.workspace_root).arg("develop");
    run_command(&mut command)
}
