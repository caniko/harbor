# js-harbor

Reusable JavaScript and Bun infrastructure for Nix flakes.

The first maintained surface wraps
[`alleneubank/bun-overlay`](https://github.com/alleneubank/bun-overlay):

- `lib.bun.mkBunPackage`
- `lib.bun.mkBunToolchain`
- `lib.bun.mkBunWorkspaceDeps`
- `lib.bun.readPackageManagerVersion`
- `packages.<system>.bun_1_3_14`

The API is intentionally namespaced under `lib.bun` so future Node, pnpm, Yarn,
and generic JavaScript helpers can be added without renaming the flake.
