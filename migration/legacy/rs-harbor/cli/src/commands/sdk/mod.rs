//! `rs-harbor sdk` — realize and publish macOS SDK archives.
//!
//! `realize` validates and resolves an SDK archive into a concrete store
//! path. `publish-macos` does the same and then pushes the resulting store
//! path to Attic.

use clap::Subcommand;

pub mod publish_macos;
pub mod realize;

pub use publish_macos::PublishMacosArgs;
pub use realize::RealizeArgs;

#[derive(Subcommand, Debug)]
pub enum SdkCommand {
    /// Realize a macOS SDK archive into a validated store path.
    Realize(RealizeArgs),
    /// Realize and publish a macOS SDK archive to Attic.
    PublishMacos(PublishMacosArgs),
}

pub fn run(cmd: SdkCommand) -> anyhow::Result<()> {
    match cmd {
        SdkCommand::Realize(args) => realize::run(args),
        SdkCommand::PublishMacos(args) => publish_macos::run(args),
    }
}

fn print_commit_block(store_path: &std::path::Path, version: &str, recursive_hash: &str) {
    print!(
        "{}",
        render_commit_block(store_path, version, recursive_hash)
    );
}

fn render_commit_block(store_path: &std::path::Path, version: &str, recursive_hash: &str) -> String {
    format!(
        "Commit this in host configuration:\n\
         ```nix\n\
         programs.rsHarbor.macosSdk.sdkVersion = \"{version}\";\n\
         programs.rsHarbor.macosSdk.storePath = \"{}\";\n\
         programs.rsHarbor.macosSdk.outputHash = \"{recursive_hash}\";\n\
         ```\n",
        store_path.display()
    )
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::render_commit_block;

    #[test]
    fn commit_block_includes_output_hash() {
        let block = render_commit_block(
            Path::new("/nix/store/example-macosx-sdk-26.1"),
            "26.1",
            "sha256-example",
        );

        assert!(block.contains("programs.rsHarbor.macosSdk.sdkVersion = \"26.1\";"));
        assert!(block.contains(
            "programs.rsHarbor.macosSdk.storePath = \"/nix/store/example-macosx-sdk-26.1\";"
        ));
        assert!(block.contains("programs.rsHarbor.macosSdk.outputHash = \"sha256-example\";"));
    }
}
