# rs-harbor

<!-- simit:badges:start -->

![CI](https://img.shields.io/badge/CI-managed-2088ff) [![Nix](https://img.shields.io/badge/Nix-managed-5277c3)](flake.nix) [![docs](https://img.shields.io/badge/docs-enabled-6f42c1)](docs) [![crates.io](https://img.shields.io/badge/crates.io-ready-f46623)](https://crates.io/crates/harbor-cache) [![artifacts](https://img.shields.io/badge/artifacts-configured-2ea44f)](.github/workflows/release.yml)

<!-- simit:badges:end -->

Reusable Rust toolchain and cross-compilation infrastructure for Nix flakes.

`rs-harbor` packages the Nix plumbing most Rust workspaces need when targeting Linux, Windows, and macOS:

- `mkToolchain` for Rust toolchains with `crane`
- optional `mkToolchain` fleet profiles for centrally pinned stable and nightly Rust versions
- `mkCargoConfig` for generated `.cargo/config.toml`
- `mkCross` for MinGW, aarch64-linux, and optional osxcross environments
- `mkCrossPackages` for building one workspace across native, aarch64-linux, Windows, and macOS targets
- `mkBinaryRelease` and `mkReleaseBinaryPackage` for signed Codeberg binary archives and locked prebuilt consumers
- `mkPortableBinaryRelease` and `mkPortableReleaseBinaryPackage` for native applications bundled with pinned `nix-bundle`
- `mkReleaseArtifact`, `mkReleaseArchive`, and `mkReleaseBundle` for generic flat release outputs with one versioned manifest
- `mkDevShell`, `mkDocsShell`, and `mkDevShells` for consistent development shells
- `mkProjectCliShellTools` for exposing a flake-built project CLI inside direnv/dev shells
- `mkGpuRenderPin` for visual snapshot test renderer pinning
- `mkAppImage`, `mkFlatpakManifest`, `mkCoprSpec`, `mkHomebrewFormula`, and `mkAndroidFlavorTable` for packaging outputs
- `devShells.<system>.opencode-lsp` for a direnv-composable OpenCode LSP profile using the rs-harbor Rust toolchain

### More helpers

The library also exports these helpers (re-exported as `rs-harbor.lib.*`):

- `mkSteamRuntimeTools` — Steam Linux Runtime container exec + ELF/PE/Mach-O runtime-dependency auditing
- `mkAdapter` / `isHarborAdapter` — typed adapter record (e.g. Attic config) consumed by other helpers
- `mkAtticPush` — flake `app` that pushes store paths to an Attic cache via an adapter
- `mkDebPackage` — Debian `.deb` package helper
- `mkScoopManifest` — Scoop (Windows) manifest generator
- `mkChocoPackage` — Chocolatey (Windows) `.nuspec` + install-script package helper
- `mkPackageArtifactBuilder` / `mkPackageTestPlan` / `mkChocoTestEnvironment` — package-builder metadata, verification plans, and Chocolatey Vagrant test environments
- `mkDioxusWebPackage` / `mkDioxusFullstackPackage` — reproducible Dioxus 0.7 web and fullstack bundles with exact `wasm-bindgen-cli`, offline Cargo vendoring, and the shared artifact contract
- `mkDioxusBuildPlan` / `resolveWasmBindgenCli` — reusable Dioxus command planning and lockfile-to-toolchain version resolution for downstream flakes
- `mkAndroidApk` / `mkAndroidApkDevBuilder` / `mkAndroidFlavorTable` / `findLocalMavenCache` — Android Rust+Gradle packages, fix-loop staging apps, flavor expansion, and pinned Maven-cache discovery

## Prerequisites

- [Nix](https://nixos.org/) with flakes enabled
- [rust-overlay](https://github.com/oxalica/rust-overlay) applied to the `pkgs` you pass into `mkToolchain`
- A configured macOS SDK only if you want osxcross-based macOS targets

## Quick Start

```nix
{
  outputs = { self, nixpkgs, rs-harbor, flake-utils, rust-overlay, ... }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [(import rust-overlay)];
      };

      toolchain = rs-harbor.lib.mkToolchain { inherit pkgs; };
      cross = rs-harbor.lib.mkCross { inherit pkgs system; };
    in {
      devShells =
        (rs-harbor.lib.mkDevShells {
          inherit pkgs cross;
          inherit (toolchain) craneLib;
        })
        // {
          docs = rs-harbor.lib.mkDocsShell {
            inherit pkgs cross;
            inherit (toolchain) craneLib;
          };
        };
    });
}
```

This produces:

- `nix develop` for native work
- `nix develop .#docs` for mdBook documentation tooling
- `nix develop ./site#docs` for the optional Plinth-powered project-site tooling
- `nix develop .#windows` for MinGW cross builds
- `nix develop .#macos` for osxcross when a macOS SDK is configured
- `nix develop .#cross` for both Windows and macOS helpers

### Dioxus bundles

Product flakes should keep their source filtering, Cargo lockfile, and
deployment layout local, then delegate the Dioxus mechanics to rs-harbor:

```nix
rs-harbor.lib.mkDioxusWebPackage {
  inherit pkgs craneLib rustToolchain;
  src = filteredSource;
  cargoLock = ./Cargo.lock;
  pname = "my-app-dioxus";
  package = "my-app";
  wasmBindgenCli = exactWasmBindgenCli;
  webFeatures = [ "web" ];
  wasmSplit = true;
  installSubdir = "share/my-app/dioxus";
}
```

Use `mkDioxusFullstackPackage` when the Dioxus server executable is the
deployable process. It emits the server under `bin/` and the generated
`public/` tree separately. A product with an existing Axum process can use
the web builder and copy its `public/` output into that process's static
tree instead. The compatibility `mkDioxusPackage` name remains available for
one migration cycle.

### Compiler-cache policy

`rs-harbor.lib.mkBuildCachePolicy` is the canonical compiler-cache contract
for Rust, Dioxus, CMake, and cross builders. It derives a versioned namespace
from the selected build-platform `sccache` package, scopes `XDG_CACHE_HOME`
inside the sandbox wrapper, and scrubs daemon/S3 credentials before invoking
the compiler cache. Hosts provide the writable sandbox mount and transport
credentials through `nixosModules.buildCache` and `nixosModules.sccache`.
`mkToolchain` leaves this policy disabled unless the consumer explicitly sets
`cache.enable = true`, so ordinary flakes remain portable to hosts without a
managed cache transport.

Rust cache setup hooks emit `RS_HARBOR_SCCACHE_STATS_V1` JSON records. In
addition to sccache counters and rs-harbor workload metadata, every record
contains explicit `compiler` and `targetTriple` fields. The target comes from
the derivation's `CARGO_BUILD_TARGET` when present, otherwise the build
platform target; callers such as osxcross may provide it explicitly.

Cross builds must instantiate the policy with the build package set. A target
`aarch64-linux` machine is never a compiler builder: evaluate and realize its
outputs from the x86_64 Atlas/Crossbow path, and keep target-native recovery
explicit rather than silently falling back to a native target build.

## macOS SDK Init

Initialize the SDK once per host, using a host-local archive path:

```bash
nix run rs-harbor#init-macos-sdk -- /host/local/MacOSX26.1.sdk.tar.xz 26.1
```

Then pass the realized store path to `mkCross` from host configuration rather than committing host-local archive paths. Full setup details are in [docs/src/reference/macos-sdk.md](docs/src/reference/macos-sdk.md).

## Homebrew Formula Bump

After release archives exist on disk, `rs-harbor brew bump` renders a Homebrew formula with real sha256 sums:

```bash
rs-harbor brew bump \
  --tap ../homebrew-tap \
  --name my-app \
  --version 1.0.0 \
  --description "My example application" \
  --homepage https://example.com/my-app \
  --license MIT \
  --archive linux_intel=https://example.com/releases/my-app-1.0.0-x86_64-linux.tar.gz,dist/my-app-linux.tar.gz
```

Use `--stdout` to print the formula for piping, or `--push` to commit and push the tap update after rs-harbor verifies the tap has no unrelated dirty files.

## Docs

- [Getting started](docs/src/getting-started/installation.md)
- [Quick start](docs/src/getting-started/quick-start.md)
- [API summary](docs/src/SUMMARY.md)
- [mkCross](docs/src/api/cross.md)
- [mkCrossPackages](docs/src/api/cross-packages.md)
- [mkDevShell, mkDocsShell, and mkDevShells](docs/src/api/dev-shells.md)
- [Packaging](docs/src/packaging/appimage.md)
- [Android APK packages](docs/src/packaging/android-apk.md)
- [Homebrew formula](docs/src/packaging/homebrew-formula.md)
- [Package tests](docs/src/packaging/package-tests.md)
- [COPR spec](docs/src/packaging/copr.md)

Source repository: <https://codeberg.org/caniko/rs-harbor>
