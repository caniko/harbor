# anx-plugin-zenodo

Zenodo archival plugin for the anx article toolchain, maintained in the
[Harbor workspace](https://github.com/caniko/harbor).

After release, install with `cargo install anx-plugin-zenodo --version 0.1.0`.
The anx toolchain invokes the executable with a required `--plugin-context`
JSON argument containing an action, paths, and metadata. The executable reports
JSON on standard output; errors also appear on standard error and exit with
status 1. See `--help` and `src/main.rs` for the current context contract.

The implementation supports Zenodo production and sandbox endpoints, OAuth
authorization, and deposition state. Remote archival actions require the
operator's Zenodo account and credentials. This package's local tests do not
establish acceptance of a live Zenodo publication.

The package declares Rust 1.88, verified against the workspace's locked dependencies.
Package documentation: <https://docs.rs/anx-plugin-zenodo> once published.

## Validation and publication

From the Harbor workspace, run package tests, warnings-denied Clippy and rustdoc,
and `cargo package --locked -p anx-plugin-zenodo` through `direnv exec .`.
Use `treefmt --ci` for formatting. Publication requires exact TeX component
qualification and the signed `anx-plugin-zenodo/v*` package tag, followed by the
protected publisher workflow.

## License

MIT. The original `LICENSE` is bundled in the package.
