# Changelog

## [0.1.0] - 2026-10-12

First registry release of the existing Harbor cache helper library.

- Attic push and administrator token-issuance helpers with caller-supplied
  executable paths and operation options.
- JWT payload decoding, expiry status with an explicit clock, and human time
  formatting helpers. Payload decoding does not validate token signatures.
- Base64 0.23 dependency convergence, retaining the existing public API and
  deterministic expiry semantics.
- Rust 1.88 release metadata, package documentation, and bundled licenses.
