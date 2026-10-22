# harbor-evidence

Source-bound artifact acceptance and atomic evidence for the
[Harbor workspace](https://github.com/caniko/harbor).

```toml
[dependencies]
harbor-evidence = "0.1.0"
```

The package also provides the `harbor` and `harbor-verify-artifact` executables.
Install them with `cargo install harbor-evidence --version 0.1.0` after release.
Use each executable's `--help` for its supported command and argument contracts.
The CLI, artifact, and evidence implementations live in this crate; the Nix
component supplies host-specific tools and acceptance policy.

The package declares Rust 1.88. API documentation is available at
<https://docs.rs/harbor-evidence> once the release is published.

## Validation and publication

From the Harbor workspace, run `cargo test --locked -p harbor-evidence`,
warnings-denied Clippy and rustdoc, and `cargo package --locked -p harbor-evidence`
through `direnv exec .`. Formatting uses `treefmt --ci`.
Package publication follows the signed `harbor-evidence/v*` tag and protected
publisher workflow after exact core component qualification. A retained artifact
or locally passing package does not establish hosted producer or consumer acceptance.

## License

MIT OR Apache-2.0. Both license texts are bundled with the package.
