# Changelog

## [Unreleased]

### Added

- `mkPkgConfigEnv` and `mkDioxusPackage` helpers, with shape checks and
  packaging documentation.
- `mkToolchain` now handles unavailable source paths safely while inspecting
  Cargo path patches.
- `lib.mkSccacheEnv` — generate `SCCACHE_*` environment variables for
  S3-compatible sccache backends
- `nixosModules.sccache` — NixOS module for declarative sccache setup
  (`programs.rsHarbor.sccache`)
- Simit CI/release badge markers to README
- Configured canonical domain `rs-harbor.tartanoglu.com` for Codeberg Pages
  with a pre-deploy validation step
- `mkTrunkPackage` now includes `clang` and `mold` in native build inputs for
  Linux mold linker support
- `mkToolchain` now guards Crane dependency-only builds from dummifying
  `[patch.crates-io]` path crates.
- Development shells now inherit the matching Cargo configuration from
  `mkToolchain` unless the caller supplies an override.

### Changed

- `mkToolchain` compiler caching is now opt-in because the sandbox wrapper
  requires host-managed cache transport.
- Sandbox startup is idempotent, and wrapped Rust/CMake derivations no longer
  place the raw `sccache` package on `PATH`.
