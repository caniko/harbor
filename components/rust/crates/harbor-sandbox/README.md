# harbor-sandbox

Isolated Linux development previews and design variants from the
[Harbor workspace](https://github.com/caniko/harbor).

```toml
[dependencies]
harbor-sandbox = "0.1.0"
```

The library exposes `Profile`, `Session`, `Snapshot`, process `RunOptions`, and
execution receipts, together with `run`, `status`, and `stop`. A profile declares
source inputs, store tool paths, commands, runtime services, build/readiness
deadlines, and optional network, GPU, and desktop capabilities. Profiles are
trusted executable policy; callers must supply the required tools.

Application processes use disposable source copies and isolated runtime state.
They do not inherit the host HOME or bus. The Rust component's Nix module supplies
Bubblewrap and optional D-Bus/Sway tools and generates compatible profiles.
The `harbor-rs` CLI exposes the Linux lifecycle integration.

Rust 1.89 is required for the standard-library file-locking API used by the
session and process engines. Runtime qualification also requires Linux namespace
support; compiling the package does not establish namespace or desktop acceptance.
API documentation: <https://docs.rs/harbor-sandbox> once published.

## Validation and publication

From the Harbor workspace, run package tests, warnings-denied Clippy and rustdoc,
and `cargo package --locked -p harbor-sandbox` through `direnv exec .`.
Formatting uses `treefmt --ci`. The Nix component's sandbox runtime and nested
desktop checks qualify the provisioned execution contract. Publication requires
exact Rust component qualification and the signed `harbor-sandbox/v*` package
tag followed by the protected publisher workflow.

## License

MIT OR Apache-2.0. Both original license texts are bundled.
