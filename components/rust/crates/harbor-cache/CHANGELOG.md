# Changelog

## [0.1.0]

- First registry release of the existing JWT diagnostics, cache publication and
  token issuance helpers, with documentation and license texts in the archive.
- Use Base64 0.23 with regression coverage for legacy Attic JWT decoding and
  strict rejection of padded and noncanonical payloads.
- Retain injectable clocks, command options and Rust 1.88 support.
