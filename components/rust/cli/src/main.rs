use clap::{Parser, Subcommand};

mod commands;

#[derive(Parser)]
#[command(
    name = "harbor-rs",
    version,
    about = "harbor-rs build helpers",
    propagate_version = true
)]
struct Cli {
    #[command(subcommand)]
    command: TopCommand,
}

#[derive(Subcommand)]
enum TopCommand {
    /// Audit release-staged binaries for forbidden runtime dependencies.
    Audit {
        #[command(subcommand)]
        command: commands::audit::AuditCommand,
    },
    /// Cache helpers for Attic-backed store paths and tokens.
    Cache {
        #[command(subcommand)]
        command: commands::cache::CacheCommand,
    },
    /// Homebrew tap helpers for formula rendering and publishing.
    Brew {
        #[command(subcommand)]
        command: commands::brew::BrewCommand,
    },
    /// Realize and publish macOS SDK archives for osxcross consumers.
    Sdk {
        #[command(subcommand)]
        command: commands::sdk::SdkCommand,
    },
    /// Stage cargo build outputs for release distribution.
    Stage {
        #[command(subcommand)]
        command: StageCommand,
    },
    /// Steam Linux Runtime container helpers.
    #[command(name = "steam-runtime")]
    SteamRuntime {
        #[command(subcommand)]
        command: commands::steam_runtime::SteamRuntimeCommand,
    },
}

#[derive(Subcommand)]
enum StageCommand {
    /// Stage per-arch and universal macOS Mach-O outputs with dSYMs.
    Macos(commands::stage_macos::StageMacosArgs),
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        TopCommand::Audit { command } => commands::audit::run(command),
        TopCommand::Brew { command } => commands::brew::run(command),
        TopCommand::Cache { command } => commands::cache::run(command),
        TopCommand::Sdk { command } => commands::sdk::run(command),
        TopCommand::Stage { command } => match command {
            StageCommand::Macos(args) => commands::stage_macos::run(args),
        },
        TopCommand::SteamRuntime { command } => commands::steam_runtime::run(command),
    }
}
