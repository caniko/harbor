# mkCrossPackages

`mkCrossPackages` builds a single Rust workspace for several targets at once,
lifting the per-target crane boilerplate (separate `cargoArtifacts`, the right
`CARGO_BUILD_TARGET`, and the matching cross env) out of consumer flakes. It is
the reusable form of the hand-rolled cross matrix that projects such as
`rs-modde` previously kept inline.

For NixOS consumers that need an explicit build/host contract, use
`mkCrossPackageOutputs`. It preserves the flat package set and adds the stable
namespace `crossPackages.<build-system>.<host-system>`:

```nix
let
  outputs = rs-harbor.lib.mkCrossPackageOutputs {
    buildSystem = "x86_64-linux";
    hostSystem = "aarch64-linux";
    inherit pkgs craneLib cross commonArgs;
    pname = "my-service";
    targets = ["aarch64-linux"];
  };
in {
  packages = outputs.packages;
  crossPackages = outputs.crossPackages;
}
```

The helper does not make an existing host-native derivation cross-compilable;
the selected target must still be produced by `mkCrossPackages` with a real
cross toolchain.

## Signature

```nix
rs-harbor.lib.mkCrossPackages {
  pkgs;            # native build-system pkgs with rust-overlay applied
  craneLib;        # native craneLib from rs-harbor.lib.mkToolchain
  cross;           # result of rs-harbor.lib.mkCross
  pname;           # base package name, e.g. "modde"
  commonArgs;      # base crane args shared across targets; MUST include `src`
  targets ? [ "native" ];   # subset of the supported target names
  targetArgs ? {};          # optional per-target extra crane args
  toolchainArgs ? {};       # mkToolchain args for non-native targets
}
```

It returns an attrset of derivations keyed by **output attribute name**:

| target name      | output attr name          | builder                                               |
| ---------------- | ------------------------- | ----------------------------------------------------- |
| `native`         | `${pname}`                | native `craneLib.buildPackage`                        |
| `aarch64-linux`  | `${pname}-aarch64-linux`  | craneLib built from `pkgsCross.aarch64-multiplatform` |
| `windows`        | `${pname}-windows`        | native craneLib + `cross.windowsEnv` + MinGW          |
| `darwin-x86_64`  | `${pname}-darwin-x86_64`  | `cross.osxcrossRustHelpers.mkCrossBuilder`            |
| `darwin-aarch64` | `${pname}-darwin-aarch64` | `cross.osxcrossRustHelpers.mkCrossBuilder`            |

Only the requested targets' attrs are returned.

## Merge order

For each target, args are merged as:

1. `commonArgs` (must include `src`)
2. the target's `pname` and cross env (`CARGO_BUILD_TARGET`, linker/CC vars,
   `PKG_CONFIG_ALLOW_CROSS`, …)
3. `targetArgs.<target>` **last**, so consumers can inject project dependencies
   (`buildInputs`, `nativeBuildInputs`, `postInstall`, `cargoBuildExtraArgs`,
   `doCheck`, …) and override anything above.

`toolchainArgs` is applied when constructing the non-native Rust toolchain. Use
it to preserve a project's pinned channel/date or target list, for example
`toolchainArgs = { channel = "nightly"; date = "2026-02-28"; };`.

## Darwin fallback

The `darwin-x86_64` and `darwin-aarch64` targets require osxcross with a realized
macOS SDK. When `cross.osxcrossRustHelpers == null` (no SDK configured), those
outputs fall back to a `runCommand` that exits 1 at build time with a clear
message, so the attribute still evaluates and the flake stays usable on hosts
without an SDK.

## System C libraries on darwin

Unlike the `windows` and `aarch64-linux` targets — where nixpkgs offers a ready
cross package set (`pkgs.pkgsCross.mingwW64`, `cross.linuxAarch64.pkgsCross`) to
populate `buildInputs` — the osxcross darwin targets get **no system C libraries**
by default, and there is no convenient `pkgsCross.*-darwin` set to pull a cross
`openssl` (or similar) from. So any crate that links a system C library through a
`-sys` build script fails on darwin:

```
warning: openssl-sys@…: Could not find directory of OpenSSL installation
error: failed to run custom build command for `openssl-sys`
  The system library `openssl` required by crate `openssl-sys` was not found.
```

The trap is **Cargo feature unification**: the offending `-sys` crate is usually
pulled in transitively and only on `cfg(unix)` (which includes macOS), so the
break shows up on darwin while native, Windows, and Linux builds stay green. For
example, `git2`'s default `https` + `ssh` features pull `openssl-sys` and
`libssh2-sys` — you may not use them, but if any workspace member or dependency
enables them, the unified darwin build links them and fails. Diagnose with
`cargo tree -i openssl-sys` against a darwin target
(`--target aarch64-apple-darwin`) to see who pulls it.

Pick the lightest mitigation that fits:

1. **Drop the default feature that pulls it.** If you only need a subset (e.g.
   `git2` for _local_ repository operations), set
   `git2 = { version = "…", default-features = false }` in the workspace-root
   `Cargo.toml`. This removes `openssl-sys`/`libssh2-sys` from **every** target,
   not just darwin, and is usually the cleanest fix. Watch for other deps
   re-enabling the feature — unification will bring it back.
2. **Vendor the C library.** Enable the crate's `vendored` feature (e.g.
   `openssl`'s `vendored`, `libgit2-sys`'s `vendored-openssl`) so the library is
   compiled from source with the cross toolchain instead of looked up as a system
   library.
3. **Provide a cross-built library.** Build the library for the darwin target and
   hand it to the build via `targetArgs.darwin-*.buildInputs` plus the matching
   `OPENSSL_DIR` / `PKG_CONFIG_*` env. This is the most work — osxcross is not a
   nixpkgs `pkgsCross`, so there is no off-the-shelf cross package — and is rarely
   worth it when (1) or (2) apply.

## Example

```nix
let
  toolchain = rs-harbor.lib.mkToolchain { inherit pkgs; };
  cross = rs-harbor.lib.mkCross { inherit pkgs system; };

  commonArgs = {
    inherit src;
    version = "1.0.0";
    strictDeps = true;
  };

  crossPkgs = rs-harbor.lib.mkCrossPackages {
    inherit pkgs cross commonArgs;
    inherit (toolchain) craneLib;
    pname = "my-app";
    targets = [ "native" "aarch64-linux" "windows" "darwin-aarch64" ];
    targetArgs = {
      native.buildInputs = [ pkgs.openssl ];
      aarch64-linux.buildInputs = [ cross.linuxAarch64.pkgsCross.openssl ];
      windows.buildInputs = with pkgs.pkgsCross.mingwW64; [ openssl windows.pthreads ];
    };
  };
in {
  packages = crossPkgs // { default = crossPkgs."my-app"; };
}
```

See [mkCross](./cross.md) for the cross toolchain inputs (`windowsEnv`,
`linuxAarch64`, `osxcrossRustHelpers`) that this helper consumes.
