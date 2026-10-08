#![allow(clippy::needless_pass_by_value)]

use std::io::{self, Write};

use anyhow::Result;
use clap::Args;

#[derive(Args, Debug)]
pub struct TokenIssueArgs {
    /// Project name to grant access to.
    #[arg(long, value_name = "NAME")]
    pub project: String,

    /// Host to SSH into for token issuance.
    #[arg(long = "ssh-host", value_name = "HOST")]
    pub ssh_host: String,

    /// Token validity duration.
    #[arg(long, value_name = "DURATION", default_value = "2y")]
    pub validity: String,

    /// Prefix applied to the token subject.
    #[arg(
        long = "subject-prefix",
        value_name = "PREFIX",
        default_value = "rs-harbor-"
    )]
    pub subject_prefix: String,
}

pub fn run(args: TokenIssueArgs) -> Result<()> {
    let token = harbor_cache::issue_token(harbor_cache::IssueTokenOpts {
        server: &args.project,
        project: &args.project,
        ssh_host: &args.ssh_host,
        validity: &args.validity,
        subject_prefix: &args.subject_prefix,
    })?;
    io::stdout().write_all(token.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::TokenIssueArgs;
    use clap::Parser;

    #[derive(Parser)]
    struct Wrapper {
        #[command(flatten)]
        args: TokenIssueArgs,
    }

    #[test]
    fn defaults_subject_prefix_and_validity() {
        let args = Wrapper::parse_from(["rs-harbor", "--project", "proj", "--ssh-host", "host"]);
        assert_eq!(args.args.validity, "2y");
        assert_eq!(args.args.subject_prefix, "rs-harbor-");
    }
}
