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

## Node and pnpm

`lib.node` owns reusable Node/pnpm development tooling:

- `mkNodeToolchain`: selected Node and pnpm packages, a shell fragment's
  `packages` and `env`, and exact `package.json.packageManager` validation.
- `mkNodeDevShell`: compose that fragment through `harbor-meta`.
- `mkPnpmPackage`: reuse Nixpkgs' pnpm builder with a project-owned version and
  fixed-output hash, using the selected Node interpreter.
- `readPnpmVersion`: require an exact pnpm version, optionally carrying Corepack's
  integrity suffix. Ranges and mutable tags are rejected.

`templates.node` demonstrates a lean consumer. The project owns Node selection
and the pnpm version/hash; Harbor owns the implementation. For an existing pnpm
workspace, pass `packageJson = ./package.json` and supply a matching `pnpm` package
if the Nixpkgs default differs. The shell does not install dependencies, rewrite
lockfiles or use Corepack to fetch a different manager. Install locked dependencies
explicitly with the selected `pnpm install --frozen-lockfile` in isolated project
state before running the project's own tests.

Python projects should compose the existing `harbor-py` environment helpers,
rather than copy uv plumbing into this library or a consumer flake.

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
