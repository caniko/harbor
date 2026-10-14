# harbor-xtask

Reusable synchronous xtask helpers from the [Harbor workspace](https://github.com/caniko/harbor).

```toml
[dependencies]
harbor-xtask = "0.1.0"
```

`ProjectConfig::from_workspace_root` creates project configuration with empty
package, documentation-site, and Nix-package selections. Configure those fields
before running the appropriate operation. The crate exposes Cargo CI plans,
command specifications and pipeline reports, Cargo checks/builds/packages,
documentation serving, Nix commands, coverage, COPR source packaging, and release
helpers. Commands use external tools supplied by the caller's environment.

The planning types and `run_pipeline`/`run_step` separate declared commands from
execution. Errors are returned when required tools, configuration, files, or
commands fail. Release operations distinguish `ReleaseMode::DryRun` from
`ReleaseMode::Execute`; callers choose the mode explicitly.

API documentation: <https://docs.rs/harbor-xtask> once the release is published.
The package declares Rust 1.88, verified against the workspace's locked dependencies.

## Validation and publication

From the Harbor workspace, run package tests and warnings-denied Clippy and
rustdoc in `direnv exec .`, use `treefmt --ci` for formatting, and verify the
standalone archive with `cargo package --locked -p harbor-xtask`.
Publication follows the signed `harbor-xtask/v*` tag and protected publisher
workflow after exact Rust component qualification. Native checks alone do not
establish hosted release acceptance.

## License

MIT OR Apache-2.0. Both original license texts are included.
