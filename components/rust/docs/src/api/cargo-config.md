# mkCargoConfig

`mkCargoConfig` generates a `.cargo/config.toml` tuned for fast local builds and cross-target linker configuration.

## What it configures

- `mold` for Linux linker acceleration when enabled
- `lld`-compatible Rust target configuration where appropriate
- nightly-only optimizations such as shared generics and parallel frontend work
- optional Cranelift code generation for projects that have verified compatibility
- dev-profile defaults that reduce debug info and peak link memory
- extra target sections for the Rust triples you care about

## Parameters

- `pkgs` (required)
- `channel`: `"nightly"` or `"stable"`. This is not `toolchainProfile`; that
  argument belongs to `mkToolchain`. Reuse `toolchain.cargoConfig` unless you
  need a different Cargo file than the selected toolchain.
- `crossTargets`: target triples to emit configuration for
- `enableMold`
- `enableCranelift` (default: `false`)
- `enableShareGenerics`
- `enableParallelFrontend`
- `enableDevProfileOpts`
- `devCodegenUnits`
- `extraConfig`: raw TOML appended to the generated file

### `enableCranelift` (default: `false`)

Cranelift is opt-in because it does not yet support LLVM's `\x01` no-mangle
marker used by some generated C and C++ FFI bindings. Affected projects can
compile successfully but fail to link with unresolved symbols. See
[rustc_codegen_cranelift#1689](https://github.com/rust-lang/rustc_codegen_cranelift/issues/1689).

Nightly projects that have verified their dependency graph can enable it:

```nix
enableCranelift = true;
```

The generated dev profile then uses Cranelift for workspace crates and LLVM
for dependencies.

### `enableDevProfileOpts` (default: `true`)

When enabled, `mkCargoConfig` writes a `[profile.dev]` section that reduces peak codegen and link memory. These settings are intended as default wins:

- `debug = "line-tables-only"` keeps line tables for backtraces while dropping full DWARF.
- `split-debuginfo = "unpacked"` puts remaining debug info in side files on platforms that support it; Windows treats this as a no-op.
- `[profile.dev.package."*"] debug = false` drops dependency debuginfo; workspace crates keep line-table backtraces.

This flag does not set `codegen-units`. That is a separate tradeoff controlled by `devCodegenUnits`.

### `devCodegenUnits` (default: `null`)

Optional positive integer. When set to `N`, `mkCargoConfig` writes `codegen-units = <N>` under `[profile.dev]`. When `null`, no `codegen-units` line is emitted and Cargo's dev default (`256`) stands.

This is a tradeoff, not a pure win. Lower codegen-units values such as `32` or `16` make each rustc invocation handle larger codegen units. That can improve wall-clock time for some workspace crates, but it can also increase per-process peak RSS. Set a value only after measuring the project that will consume the config.

Cargo profile precedence means keys in a project's `Cargo.toml [profile.dev]` override the same keys from this generated `.cargo/config.toml`. Set `enableDevProfileOpts = false` and `devCodegenUnits = null` if a project needs Cargo's full default dev profile.

Both parameters are pure `.cargo/config.toml` annotations. They do not add binaries to the build environment.

## Returns

- `configText`: rendered TOML
- `configPath`: store path to the generated config file

## Example

```nix
cargoConfig = harbor-rs.lib.mkCargoConfig {
  inherit pkgs;
  extraConfig = ''
    [alias]
    xtask = "run -p xtask --"
  '';
};
```

Pass the result into `mkDevShell` or `mkDevShells` and `harbor-rs` will install
it into a hash-specific directory under the user cache. That directory is
exported as `CARGO_HOME` only when the caller has not already selected one, so
the project tree stays free of generated configuration.
