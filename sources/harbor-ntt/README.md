# harbor-ntt

Reusable, pinned Wormhole Native Token Transfer toolchains and offline build
infrastructure for Nix flakes.

Consumers own all source, toolchain, and audit pins. `harbor-ntt` supplies the
reproducible CLI, EVM, and Solana builders plus an isolated development shell;
it does not own deployment configuration, credentials, expected program IDs, or
artifact acceptance policy.

```nix
ntt = inputs.harbor-ntt.lib.mkNtt {
  inherit pkgs pins anchorRustToolchain;
};
```
