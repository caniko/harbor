use std::process::Command;

use anyhow::Result;

use crate::{CoverageMode, ProjectConfig, run_command};

pub fn run_coverage(cfg: &ProjectConfig, mode: CoverageMode) -> Result<()> {
    match mode {
        CoverageMode::Summary => {
            let mut command = coverage_command(cfg);
            command.arg("--summary-only");
            run_command(&mut command)
        }
        CoverageMode::Html => {
            let mut command = coverage_command(cfg);
            command.arg("--html");
            run_command(&mut command)
        }
        CoverageMode::Lcov => {
            let mut command = coverage_command(cfg);
            command
                .arg("--lcov")
                .arg("--output-path")
                .arg("target/llvm-cov/lcov.info");
            run_command(&mut command)
        }
        CoverageMode::Ci { fail_under_lines } => {
            let target_dir = cfg.workspace_root.join("target/llvm-cov");
            std::fs::create_dir_all(&target_dir)?;

            let mut collect = coverage_command(cfg);
            collect.arg("--no-report");
            run_command(&mut collect)?;

            let mut lcov = coverage_report_command(cfg);
            lcov.arg("--lcov")
                .arg("--output-path")
                .arg("target/llvm-cov/lcov.info");
            run_command(&mut lcov)?;

            let mut summary = coverage_report_command(cfg);
            summary
                .arg("--summary-only")
                .arg("--fail-under-lines")
                .arg(fail_under_lines.to_string());
            run_command(&mut summary)
        }
    }
}

fn coverage_command(cfg: &ProjectConfig) -> Command {
    let mut command = Command::new("cargo");
    command
        .current_dir(&cfg.workspace_root)
        .arg("llvm-cov")
        .arg("--workspace")
        .arg("--all-features");
    command
}

fn coverage_report_command(cfg: &ProjectConfig) -> Command {
    let mut command = Command::new("cargo");
    command
        .current_dir(&cfg.workspace_root)
        .arg("llvm-cov")
        .arg("report");
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ci_mode_creates_report_path_under_target() {
        let cfg = ProjectConfig::from_workspace_root("/tmp/project");
        let mut command = coverage_report_command(&cfg);
        command
            .arg("--lcov")
            .arg("--output-path")
            .arg("target/llvm-cov/lcov.info");

        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert!(args.contains(&String::from("target/llvm-cov/lcov.info")));
    }
}
