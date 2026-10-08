use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use harbor_xtask::{
    CargoWorkspace, CheckProfile, ProjectConfig, cargo_ci_plan_with_nextest_args,
    load_harbor_ci_config, write_report,
};

fn main() -> Result<()> {
    let mut profile = CheckProfile::Default;
    let mut keep_going = false;
    let mut report_path = None;
    let mut args = env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "fast" => profile = CheckProfile::Fast,
            "default" => profile = CheckProfile::Default,
            "full" => profile = CheckProfile::Full,
            "--keep-going" => keep_going = true,
            "--report" => {
                report_path = Some(PathBuf::from(
                    args.next().context("--report requires a path")?,
                ));
            }
            "--help" | "-h" => {
                println!("Usage: harbor-ci [fast|default|full] [--keep-going] [--report PATH]");
                return Ok(());
            }
            other => bail!("unknown harbor-ci argument `{other}`; use --help"),
        }
    }

    let root = env::current_dir().context("reading current directory")?;
    let config = load_harbor_ci_config(&root)?;
    let mut project = ProjectConfig::from_workspace_root(&root);
    project.cargo_workspace = CargoWorkspace {
        packages: config.packages.clone(),
        excludes: config.excludes.clone(),
        all_features: config.all_features,
    };

    let plan = cargo_ci_plan_with_nextest_args(
        &project,
        profile,
        config.cargo_options(),
        &config.nextest_args,
    )
    .keep_going(keep_going);
    let report = plan.run()?;
    if let Some(path) = report_path {
        write_report(path, &report)?;
    }
    Ok(())
}
