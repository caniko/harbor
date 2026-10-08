use std::process::{Command, Output};

use anyhow::{Context, Result, bail};

/// Options for issuing an Attic token over SSH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IssueTokenOpts<'a> {
    pub server: &'a str,
    pub project: &'a str,
    pub ssh_host: &'a str,
    pub validity: &'a str,
    pub subject_prefix: &'a str,
}

/// Issue a fresh token by running `ssh <ssh_host> sudo atticadm make-token ...`.
///
/// # Errors
///
/// Returns an error if `ssh` cannot be spawned, if it exits unsuccessfully,
/// or if the command produces empty stdout.
pub fn issue_token(opts: IssueTokenOpts<'_>) -> Result<String> {
    let output = Command::new("ssh")
        .arg(opts.ssh_host)
        .arg("sudo")
        .arg("atticadm")
        .arg("make-token")
        .arg("--sub")
        .arg(format!("{}{}", opts.subject_prefix, opts.project))
        .arg("--pull")
        .arg(opts.project)
        .arg("--push")
        .arg(opts.project)
        .arg("--validity")
        .arg(opts.validity)
        .output()
        .with_context(|| format!("running ssh against {}", opts.ssh_host))?;

    ensure_success("ssh", &output)?;

    let token = String::from_utf8(output.stdout).context("decoding ssh stdout")?;
    let token = token.trim().to_owned();
    if token.is_empty() {
        bail!(
            "ssh {} returned empty stdout while issuing token for {}",
            opts.ssh_host,
            opts.server
        );
    }
    Ok(token)
}

fn ensure_success(program: &str, output: &Output) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    bail!("{program} exited with {}: {}", output.status, stderr.trim())
}
