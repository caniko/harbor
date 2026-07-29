# Changelog

## [0.1.1] - 2026-07-29

- Make the signed GitHub release bundle evaluate without developer-local flake inputs.

## [0.1.0] - 2026-07-29

- Initial signed GitHub release bundle for the rs-harbor CLI.

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
