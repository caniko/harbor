# Changelog

## [0.1.0] - Unreleased

### Added

- Realize and validate macOS SDK archives through the external SDK helpers.
- Publish realized SDK store paths to Attic using a temporary configuration.
- Register optional indirect Nix GC roots before publication.

### Changed

- Publish under `caniko-harbor-sdk` because an unrelated crates.io project owns
  `harbor-sdk`. The Rust library name `harbor_sdk` and Harbor CLI dependency alias
  remain stable.
- Bundle package documentation and the original MIT and Apache-2.0 license texts.
