#![allow(clippy::needless_pass_by_value)]

use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

#[derive(Args, Debug)]
pub struct PushArgs {
    /// Store path to push into Attic.
    #[arg(value_name = "store-path")]
    pub store_path: PathBuf,

    /// Cache name to push into.
    #[arg(long, value_name = "NAME")]
    pub cache: String,

    /// Attic server URL.
    #[arg(long, value_name = "URL")]
    pub server: Option<String>,

    /// Path to a file containing the Attic token.
    #[arg(long = "token-file", value_name = "PATH")]
    pub token_file: Option<PathBuf>,
}

pub fn run(args: PushArgs) -> Result<()> {
    let opts = harbor_cache::PushOpts {
        cache: &args.cache,
        store_path: &args.store_path,
        server: args.server.as_deref(),
        token_file: args.token_file.as_deref(),
    };
    harbor_cache::push(opts)
}
