# js-harbor

Reusable JavaScript and Bun infrastructure for Nix flakes.

The first maintained surface wraps
[`alleneubank/bun-overlay`](https://github.com/alleneubank/bun-overlay):

- `lib.bun.mkBunPackage`
- `lib.bun.mkBunFhsRunner`
- `lib.bun.mkBunToolchain`
- `lib.bun.mkBunWorkspaceDeps`
- `lib.bun.readPackageManagerVersion`
- `packages.<system>.bun_1_3_14`

On Linux, `mkBunPackage` keeps the upstream Bun binary unpatched and runs it
through `proot` so `bun build --compile` keeps working in Nix build sandboxes.
The package exposes `bun.passthru.fhsRunner` for running Bun-compiled Linux
executables during checks or install hooks.

The API is intentionally namespaced under `lib.bun` so future Node, pnpm, Yarn,
and generic JavaScript helpers can be added without renaming the flake.
