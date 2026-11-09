use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use harbor_cli::align::{AlignReport, align_locks};
use harbor_cli::bump::{BumpOptions, BumpReport, bump};
use harbor_cli::mcp;

#[derive(Parser)]
#[command(name = "harbor", version, about = "Harbor superproject maintenance")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Align component locks with the root flake.lock
    AlignLocks {
        /// Report drift and exit non-zero instead of writing
        #[arg(long)]
        check: bool,
        /// Harbor superproject checkout
        #[arg(long)]
        root: Option<PathBuf>,
        /// Root inputs to align (default: every shared input)
        #[arg(value_name = "INPUT")]
        inputs: Vec<String>,
    },
    /// Move a root flake input to a new revision
    Bump {
        /// Root input name, for example nixpkgs
        input: String,
        /// Target 40-character lowercase hex revision
        #[arg(long)]
        rev: String,
        /// Harbor superproject checkout (default: the current git toplevel)
        #[arg(long)]
        root: Option<PathBuf>,
        /// Branch name (default: build/<input>-<rev7>)
        #[arg(long)]
        branch: Option<String>,
        /// Push each branch to origin
        #[arg(long)]
        push: bool,
        /// Open a pull request for each branch
        #[arg(long)]
        pr: bool,
        /// Print the plan without writing or running nix/git mutations
        #[arg(long)]
        plan: bool,
        /// Print one JSON object
        #[arg(long)]
        json: bool,
    },
    /// Serve Harbor operations over MCP on stdio
    Mcp {
        /// Harbor superproject checkout; tool arguments cannot change it
        #[arg(long)]
        root: PathBuf,
        /// Allow clients to push branches and open pull requests
        #[arg(long)]
        allow_publish: bool,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("harbor: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode> {
    match Cli::parse().command {
        Command::AlignLocks {
            check,
            root,
            inputs,
        } => align_command(check, root, &inputs),
        Command::Bump {
            input,
            rev,
            root,
            branch,
            push,
            pr,
            plan,
            json,
        } => {
            let options = BumpOptions {
                root: resolve_root(root)?,
                input,
                rev,
                branch,
                push,
                pr,
                plan,
            };
            let report = bump(&options)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_bump(&report);
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Mcp {
            root,
            allow_publish,
        } => {
            mcp::serve(root, allow_publish)?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn align_command(check: bool, root: Option<PathBuf>, inputs: &[String]) -> Result<ExitCode> {
    let root = root.unwrap_or_else(|| PathBuf::from("."));
    let report = align_locks(&root, inputs, !check)?;
    print_align(&report);
    if check && !report.is_clean() {
        return Ok(ExitCode::from(1));
    }
    Ok(ExitCode::SUCCESS)
}

fn print_align(report: &AlignReport) {
    if report.is_clean() {
        println!("component locks already match the root");
    } else {
        for line in &report.changed {
            println!("{line}");
        }
    }
}

fn resolve_root(root: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = root {
        path.canonicalize()
            .with_context(|| format!("resolve root {}", path.display()))
    } else {
        let output = std::process::Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .context("run git rev-parse --show-toplevel")?;
        if !output.status.success() {
            anyhow::bail!(
                "not inside a git checkout: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(PathBuf::from(
            String::from_utf8_lossy(&output.stdout).trim(),
        ))
    }
}

fn print_bump(report: &BumpReport) {
    let action = if report.plan { "plan" } else { "bump" };
    println!(
        "{action}: {} {} -> {}",
        report.input, report.root.old, report.root.new
    );
    for component in &report.components {
        let branch = component
            .branch
            .as_deref()
            .map(|branch| format!(" branch {branch}"))
            .unwrap_or_default();
        match (&component.commit, &component.change) {
            (Some(commit), _) => println!("  {:<12} {commit}{branch}", component.name),
            (None, Some(change)) => println!("  {:<12} {change}", component.name),
            (None, None) => println!("  {}", component.name),
        }
        if let Some(url) = &component.pr_url {
            println!("  {:<12} {url}", "");
        }
    }
    if report.plan {
        if let Some(files) = &report.superproject.files {
            println!("superproject files: {}", files.join(" "));
        }
    } else if let Some(commit) = &report.superproject.commit {
        let branch = report
            .superproject
            .branch
            .as_deref()
            .map(|branch| format!(" branch {branch}"))
            .unwrap_or_default();
        println!("superproject {commit}{branch}");
        if let Some(url) = &report.superproject.pr_url {
            println!("superproject pr {url}");
        }
    }
}
