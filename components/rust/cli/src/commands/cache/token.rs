use clap::Subcommand;

pub mod issue;

#[derive(Subcommand, Debug)]
pub enum TokenCommand {
    /// Issue a fresh Attic token for a project.
    Issue(issue::TokenIssueArgs),
}

pub fn run(cmd: TokenCommand) -> anyhow::Result<()> {
    match cmd {
        TokenCommand::Issue(args) => issue::run(args),
    }
}
