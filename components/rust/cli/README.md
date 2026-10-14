# nix_harbor_rs

The `harbor-rs` command-line build helpers from the
[Harbor workspace](https://github.com/caniko/harbor).

After release, install with `cargo install nix_harbor_rs --version 0.1.0`.
The registry package is `nix_harbor_rs`; the executable is `harbor-rs`.
Use `harbor-rs --help` and each subcommand's help for the current operator surface.
Commands rely on the external tools provided by the Harbor development environment.

The package consumes `harbor-cache` for cache helpers, `caniko-harbor-sdk` under
the `harbor-sdk` dependency alias for macOS SDK orchestration, and, on Linux,
`harbor-sandbox` for isolated development sessions. Their registry releases are
prerequisites for packaging the CLI without local path dependencies.

The compiler requirement is Rust 1.89, including the Linux sandbox dependency's
standard-library file-locking API. The Nix component retains native macOS builds
and Linux-only sandbox command selection. Package documentation:
<https://docs.rs/nix_harbor_rs> once published.

## Validation and publication

From the Harbor workspace, run `cargo test --locked -p nix_harbor_rs`,
warnings-denied Clippy and rustdoc through `direnv exec .`; formatting uses
`treefmt --ci`. After dependency releases, verify the standalone archive with
`cargo package --locked -p nix_harbor_rs`.
Publication requires exact Rust component qualification and the signed
`nix_harbor_rs/v*` package tag followed by the protected publisher workflow.

## License

MIT OR Apache-2.0. Both original license texts are bundled.
