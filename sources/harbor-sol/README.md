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
