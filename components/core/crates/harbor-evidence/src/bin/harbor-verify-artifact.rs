use clap::Parser;

fn main() -> anyhow::Result<()> {
    harbor_evidence::cli::run_compatibility(harbor_evidence::cli::CompatibilityCli::parse())
}
