# harbor-js

Reusable JavaScript and Bun infrastructure for Nix flakes.

The first maintained surface wraps
[`alleneubank/bun-overlay`](https://github.com/alleneubank/bun-overlay):

- `lib.bun.mkBunPackage`
- `lib.bun.mkBunFhsRunner`
- `lib.bun.mkBunToolchain`
- `lib.bun.mkBunDevShell`
- `lib.bun.mkBunWorkspaceDeps`
- `lib.bun.readPackageManagerVersion`
- `packages.<system>.bun_1_3_14`
- `packages.x86_64-linux.bun_1_3_14_baseline`

On Linux, `mkBunPackage` keeps the upstream Bun binary unpatched and runs it
through `proot` so `bun build --compile` keeps working in Nix build sandboxes.
The package exposes `bun.passthru.fhsRunner` for running Bun-compiled Linux
executables during checks or install hooks.

The API is intentionally namespaced under `lib.bun` so future Node, pnpm, Yarn,
and generic JavaScript helpers can be added without renaming the flake.

```bash
nix flake init -t github:caniko/harbor-js
```

## Project dependencies

The dev shell supplies Bun, not the project's `node_modules`. Follow the
[shared agent-shell guidance](https://github.com/caniko/harbor-meta#agent-shells-and-validation)
and install dependencies explicitly from the project root:

```bash
direnv exec . bun install --frozen-lockfile
```

Commit the project's lockfile before treating installs as reproducible. A
lockfile-less vendored project needs its own locking policy; an unpinned local
install is not a reproducible setup. Do not add network installs to shell hooks.
After installation, run the project's documented startup/import smoke check.
For a CLI that supports it, a help command such as
`direnv exec . bun run src/cli.ts --help` checks startup without running a job.

For Nix-managed dependency artifacts, reuse `lib.bun.mkBunWorkspaceDeps` with a
locked source tree and the expected output hash rather than writing another
installer. This does not automatically populate a working checkout's dependencies.
