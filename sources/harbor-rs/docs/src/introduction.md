# harbor-rs

`harbor-rs` is reusable Rust cross-compilation and toolchain infrastructure for Nix flakes. It packages the common setup needed for Rust workspaces that target Linux, Windows, and macOS without forcing each project to rebuild the same Nix plumbing.

The library exposes:

- `mkToolchain` for Rust toolchains with `crane`
- `mkCargoConfig` for generated `.cargo/config.toml`
- `mkCross` for MinGW and optional osxcross environments
- `mkDevShell` and `mkDevShells` for consistent development shells
- `mkGpuRenderPin` for GPU/driver identity checks in visual snapshot tests
- `mkAppImage` and `mkFlatpakManifest` for packaging outputs
- `mkAdapter` and `mkAtticPush` for binary cache integration

Start with [Installation](./getting-started/installation.md) if you want to consume the flake in another project, or jump to [Quick Start](./getting-started/quick-start.md) for a minimal cross-compiling shell.

Source repository: <https://github.com/caniko/harbor-rs>
