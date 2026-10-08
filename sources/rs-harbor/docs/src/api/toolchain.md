# mkToolchain

`mkToolchain` creates the Rust toolchain and `craneLib` used by the rest of the flake.

## Parameters

- `pkgs` (required): nixpkgs with `rust-overlay` applied
- `toolchainProfile`: optional rs-harbor-owned pin, either `"stable"` or `"nightly"`. Stable is currently pinned to Rust `1.97.1`; nightly uses the repository's checked-in `rust-toolchain.toml`. Omitting it preserves the legacy channel/date behavior.
- `toolchainFile`: optional path to a standard `rust-toolchain.toml`; when set, its channel, components, and targets are authoritative
- `channel`: `"nightly"` or `"stable"`; defaults to `"nightly"`
- `date`: `"latest"` or a pinned date such as `"2025-12-01"`; only used without `toolchainFile`
- `extensions`: extra Rust components to install. Defaults to `["rust-src" "rustfmt" "rustc-codegen-cranelift-preview" "llvm-tools-preview"]`. `llvm-tools-preview` provides `llvm-cov`/`llvm-profdata`, which `cargo-llvm-cov`-based coverage CI requires.
- `withRustAnalyzer`: whether to include `rust-analyzer` in the toolchain extensions; defaults to `true`
- `crossTargets`: list of target triples to include in the toolchain
- `cache.enable`: explicitly enable the host-backed compiler cache; defaults to `false`

## Returns

`mkToolchain` returns an attribute set with:

- `rustToolchain`
- `craneLib`
- `buildCache`: the compiler-cache policy when enabled, otherwise `null`
- `cargoConfig`: matching `mkCargoConfig` output, inherited automatically by rs-harbor dev shells using this `craneLib`
- `crossTargets`

## Path-patched crates and `buildDepsOnly`

The returned `craneLib` wraps Crane's `buildPackage` and `buildDepsOnly` for workspaces that use `[patch.crates-io]` with local `path` entries.

Crane's dependency-only phase builds a dummy source tree for path crates. That is unsafe when a registry dependency compiles against a patched local crate, because the dependency may see the dummy crate API instead of the real patched API.

For these workspaces, `craneLib.buildPackage` automatically disables implicit dependency artifact reuse by passing `cargoArtifacts = null` unless the caller already provided `cargoArtifacts`.

Direct `craneLib.buildDepsOnly` calls fail with an rs-harbor error naming the path patches. Prefer:

```nix
craneLib.buildPackage (commonArgs // {
  cargoArtifacts = null;
})
```

If a workspace is known to tolerate dummy path patches, pass `rsHarborAllowPathPatchBuildDepsOnly = true` to `buildDepsOnly`.

Path-patch detection reads `Cargo.toml` during evaluation. Direct Nix paths and
`lib.cleanSourceWith` sources are handled automatically. If `src` is a generated
derivation output or an undeclared plain-string path, provide the manifest
explicitly to avoid import-from-derivation and store-state-dependent evaluation:

```nix
craneLib.buildPackage {
  src = generatedSource;
  rsHarborCargoTomlContents = builtins.readFile ./Cargo.toml;
  # ...
}
```

The rs-harbor-only argument is removed before the remaining arguments are
passed to Crane.

## Example

```nix
toolchain = rs-harbor.lib.mkToolchain {
  inherit pkgs;
  channel = "stable";
  date = "2026-04-01";
};
```

Most downstream projects only need `craneLib` from the result. [mkDevShell and mkDevShells](./dev-shells.md) inherit its matching Cargo configuration automatically.

Compiler caching is host infrastructure rather than a portable toolchain
default. Hosts with a managed cache transport can opt in explicitly:

```nix
toolchain = rs-harbor.lib.mkToolchain {
  inherit pkgs;
  cache.enable = true;
};
```

## Optional fleet profiles

The profiles are opt-in. A project that wants rs-harbor to control its Rust
version can select a profile and reuse the returned Cargo configuration:

```nix
toolchain = rs-harbor.lib.mkToolchain {
  inherit pkgs;
  toolchainProfile = "stable";
};
cargoConfig = toolchain.cargoConfig;
```

The selected profile is also inherited by `mkCrossPackages` for non-native
outputs unless `toolchainArgs` is supplied explicitly. Updating the profile
manifest in rs-harbor then updates every consumer when it refreshes its
rs-harbor input, without requiring per-project version edits.

Projects with a checked-in Rust toolchain file can consume it directly:

```nix
toolchain = rs-harbor.lib.mkToolchain {
  inherit pkgs;
  toolchainFile = ./rust-toolchain.toml;
};
```

In file mode, `channel` and `date` must be omitted. Explicit `extensions` and
`crossTargets` are added to the components and targets declared by the file.
