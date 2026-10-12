use clap::Parser;

fn main() -> anyhow::Result<()> {
    harbor_evidence::cli::run(harbor_evidence::cli::Cli::parse())
}
