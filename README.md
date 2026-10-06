# harbor-sol

Reusable Solana, Agave validator, and Anchor development infrastructure for
Nix flakes. Rust toolchain ownership remains in `harbor-rs`; shell composition
comes from `harbor-meta`.

```sh
nix flake init -t github:caniko/harbor-sol
nix develop
anchor build --ignore-keys
solana-test-validator
```

`mkCargoBuildSbf` reads the SDK manifests from the locked `solana-source`
non-flake input, so evaluating packages and shells does not require building a
source-fetcher derivation. The source version must match the selected
`solana-cli`. When overriding `solana`, pass a matching tracked `solanaSource`
to `mkCargoBuildSbf` and supply that result as `cargoBuildSbf` to the shell.
