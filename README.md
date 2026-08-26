# harbor-eth

Reusable Foundry and Solidity development infrastructure for Nix flakes.

`harbor-eth` uses the packaged `foundry` and `solc` toolchains and composes its
dev shells through `harbor-meta`. Foundry already provides the local `anvil`
test backend, so no separate service wrapper is required.

```sh
nix flake init -t github:caniko/harbor-eth
nix develop
forge test
```
