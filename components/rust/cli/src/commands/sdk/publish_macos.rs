//! `harbor-rs sdk publish-macos` — realize a macOS SDK archive and push the
//! resulting store path to Attic.

use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

#[derive(Args, Debug)]
// `--version` is a real SDK-version argument here, not a version flag; drop
// clap's auto-generated `--version` so the two don't collide.
#[command(disable_version_flag = true)]
pub struct PublishMacosArgs {
    /// SDK archive path produced by the host-local Apple SDK download.
    #[arg(long)]
    pub archive: PathBuf,

    /// macOS SDK version to realize, such as `26.1`.
    #[arg(long)]
    pub version: String,

    /// Attic server URL, e.g. `https://attic.example.org`.
    #[arg(long)]
    pub attic_server: String,

    /// Attic cache name to push the realized store path into.
    #[arg(long)]
    pub cache: String,

    /// Path to the Attic token file.
    #[arg(long)]
    pub token_file: PathBuf,

    /// Register a Nix GC root at this link pointing to the realized store
    /// path on the publishing host, so it survives `nix-collect-garbage`.
    /// Created with `nix-store --add-root --indirect`.
    #[arg(long)]
    pub gc_root: Option<PathBuf>,
}

pub fn run(
    PublishMacosArgs {
        archive,
        version,
        attic_server,
        cache,
        token_file,
        gc_root,
    }: PublishMacosArgs,
) -> Result<()> {
    let realized = harbor_sdk::publish_macos_sdk(harbor_sdk::PublishOpts {
        archive,
        version,
        attic_server,
        cache_name: cache,
        token_file,
        gc_root,
    })?;
    super::print_commit_block(
        &realized.store_path,
        &realized.version,
        &realized.recursive_hash,
    );
    Ok(())
}
