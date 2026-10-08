# Modular Formatting

Harbor exports plain treefmt-nix modules under `treefmtModules`. Import only
the languages you need. Language modules do not import a base module, select
a project root, install toolchain overlays, or exclude consumer directories.

| Owner | Export | Formatter |
| --- | --- | --- |
| harbor-meta | `treefmtModules.nix` | Alejandra |
| harbor-meta | `treefmtModules.toml` | Taplo |
| harbor-rs | `treefmtModules.rust` | rustfmt |
| harbor-js | `treefmtModules.javascript` | Prettier for JS, TS, and JSON |
| harbor-py | `treefmtModules.python` | Ruff format, not lint/fix |
| harbor-android | `treefmtModules.java` | google-java-format |
| harbor-android | `treefmtModules.kotlin` | ktfmt |
| harbor-eth | `treefmtModules.solidity` | Forge fmt |
| harbor-tex | `treefmtModules.latex` | latexindent |

`harbor-db` and `harbor-sol` compose the Rust module, not copies of it.
`harbor-ntt` formats its Nix/TOML infrastructure; NTT consumers explicitly
select Rust, JS, and Solidity modules for their sources. The input-free
`harbor-macos-sdk-pin` is a data flake, not a toolchain Harbor, and stays
input-free.

## Consumer Composition

Use one treefmt-nix evaluator and one consumer package set. For example,
inside a per-system flake output:

```nix
let
  treefmt = inputs.treefmt-nix.lib.evalModule pkgs {
    imports = [
      inputs.harbor-meta.treefmtModules.nix
      inputs.harbor-meta.treefmtModules.toml
      inputs.harbor-rs.treefmtModules.rust
      inputs.harbor-js.treefmtModules.javascript
    ];
    projectRootFile = "flake.nix";
    settings.global.excludes = ["generated/**"];
    programs.rustfmt.package = toolchain.rustToolchain;
    programs.rustfmt.edition = "2024";
  };
in {
  formatter = treefmt.config.build.wrapper;
  checks.formatting = treefmt.config.build.check self;
}
```

The Rust module leaves package and edition selection to treefmt-nix defaults
unless the consumer overrides them. Rust templates retain their explicit
editions and toolchains. A repository with several Rust editions should
configure path-specific formatters instead of assuming one global edition.

Prettier does not claim TOML, Nix, Markdown, or YAML files. Enable those
separately if needed. Keep generated-file exclusions in the consumer;
the JS template preserves its `.crow/**` exclusion there.

The repository formatter outputs and the Rust, Python, JavaScript, TeX,
Android, Ethereum, and Solana templates use explicit module composition.
Existing Simit-managed consumers are not rewritten by this change.

## Direnv

Load a consumer's dev shell with its `.envrc` (`use flake . || return 1`).
The configured treefmt hook declares `package = treefmtWrapper`, so shells
including `pre-commit-check.enabledPackages` put the same configured wrapper
on `PATH` for interactive use. Projects without that hook plumbing must add
their formatter output to their shell's packages explicitly; Canix's
configure shell does this directly.

```sh
treefmt
treefmt --fail-on-change
```

Outside an interactive direnv-enabled shell, use `direnv exec . treefmt`.
The formatter itself invokes neither Nix nor direnv. Loading or refreshing
the dev shell still evaluates Nix and may realize dependencies; this is not
a promise that a cold direnv load is build-free. Do not automatically allow
new `.envrc` files on behalf of the operator.

## Checks

`harbor-meta` exposes `checks.<system>.treefmt-modules`, testing independence,
import-order and duplicate-import equivalence, consumer exclusions, real
Nix/TOML formatting, and a second uncached pass for idempotence.
`harbor-rs` exposes the same check name for Rust-only isolation, consumer
toolchain/edition overrides, mixed Nix/Rust/TOML composition, and formatting
idempotence. JS, Python and TeX template checks inspect evaluated formatter
settings rather than source-string declarations.

```sh
nix build .#checks.x86_64-linux.treefmt-modules --no-link
```

## Coordinated Rollout

These exports must exist in each locked Harbor input before a consumer can
evaluate its new composition. Publish the scoped harbor-meta changes first,
then update dependent Harbor locks. Publish harbor-rs before updating the
explicit Rust pin in harbor-db and the Rust input in harbor-sol. Do not
commit local `path:` overrides or lock unrelated dirty changes into a release.

During development, validate with temporary overrides and no lock writes:

```sh
# From a sibling harbor-rs checkout:
nix build .#checks.x86_64-linux.treefmt-modules --no-link \
  --override-input harbor-meta path:../harbor-meta --no-write-lock-file
```

Run module evaluations on each repository's declared systems; building a
Linux formatter does not prove Darwin execution. Lock refreshes and runtime
checks are required before calling the coordinated rollout complete.

## Agent Formatter Policy

LLM agents must format through `treefmt` — locally, or fleet-wide through
`canix workspace format` — and never invoke a formatter directly.
`harbor-opencode sync` renders the deny fragment into each project's
`.opencode/opencode.jsonc` under `permission.bash`;
`harbor-opencode check` and `harbor-opencode rollout [--check]` verify it.
Detection is profile-driven: a Harbor binds its profile registry into its
own `mkCli`, and each profile declares the root files and `flake.nix`
markers that select it. The profile-less build shipped by harbor-meta
resolves `detect` to `none` — a policy-only config (the deny fragment
without any `lsp` block) — so coverage is language-independent, and a
project matching no profile renders the same policy-only config.

Harbor owns this project-side policy. The Home Manager global policy
(`agent_safety.nix`) is deliberately untouched and still allows several of
these tools: opencode concatenates permissions across config documents and
evaluates them last-match-wins over the cascade `global < explicit < direct
< project < content`, so the project deny beats a global allow regardless of
alphabet; a matching deny is also checked before any session "always allow"
approval. Unmatched commands fall through to `ask`, which is why the fragment
has no `"*": "deny"` or `"*": "ask"` catch-all.

Each pattern is rendered bare, with an `*=* ` environment prefix (the
deployed matcher has no dedicated assignment parser, so the prefix must be
spelled out), and — unless `storePathTwins = false` — through
`/nix/store/*/bin/…` and `./result/bin/…`, both plain and `*=* `-prefixed.
The `ruff check *--fix*` rule is argument-aware: a plain `ruff check .` lint
stays allowed. Multi-purpose tools are denied only at their formatting
subcommand, so build, test, plan, and lint invocations are unaffected.

| Denied pattern | Owner |
| --- | --- |
| `alejandra *` | harbor-meta `treefmtModules.nix` (Alejandra) |
| `taplo *` | harbor-meta `treefmtModules.toml` (Taplo) |
| `rustfmt *` | harbor-rs `treefmtModules.rust` (rustfmt) |
| `cargo fmt *` | harbor-rs `treefmtModules.rust` (rustfmt via cargo) |
| `prettier *` | harbor-js `treefmtModules.javascript` (Prettier) |
| `npx prettier *` | harbor-js `treefmtModules.javascript` (exec twin) |
| `pnpm exec prettier *` | harbor-js `treefmtModules.javascript` (exec twin) |
| `pnpm dlx prettier *` | harbor-js `treefmtModules.javascript` (exec twin) |
| `yarn dlx prettier *` | harbor-js `treefmtModules.javascript` (exec twin) |
| `bunx prettier *` | harbor-js `treefmtModules.javascript` (exec twin) |
| `ruff format *` | harbor-py `treefmtModules.python` (Ruff format) |
| `ruff check *--fix*` | harbor-py `treefmtModules.python` (lint fix writes) |
| `google-java-format *` | harbor-android `treefmtModules.java` |
| `ktfmt *` | harbor-android `treefmtModules.kotlin` |
| `forge fmt *` | harbor-eth `treefmtModules.solidity` (Forge fmt) |
| `latexindent *` | harbor-tex `treefmtModules.latex` |
| `deadnix *` | fleet treefmt programs (canix-toolbelt `flake-modules/formatters.nix`) |
| `gofmt *` | fleet treefmt programs (canix-toolbelt `flake-modules/formatters.nix`) |
| `just --fmt *` | fleet treefmt programs (canix-toolbelt `flake-modules/formatters.nix`) |
| `statix *` | fleet treefmt programs (canix-toolbelt `flake-modules/formatters.nix`) |
| `statix-fix *` | fleet treefmt programs (treefmt invokes statix via the `statix-fix` wrapper binary) |
| `terraform fmt *` | fleet treefmt programs (canix-toolbelt `flake-modules/formatters.nix`) |
| `shfmt *` | root policy (`agent_safety.nix`; treefmt `--fail-on-change` is the gate) |
| `shfmt -d *` | root policy (the globally allowed read-only shape) |
| `tofu fmt *` | root policy (OpenTofu formatting) |
| `go fmt *` | root policy (Go formatting) |
| `nix fmt *` | root policy (`nix fmt` realises arbitrary flake outputs) |

Every Harbor-rendered config also carries the top-level marker
`"harbor.meta/opencode-config": "1"`. `harbor-opencode sync` refuses to
overwrite an existing config that lacks the marker (a hand-written config)
unless `--force` is passed, and reports such files as `custom` during
`rollout`. The check requires that exact key at the top level with that
exact value: a config that only mentions the marker inside a comment,
nests it deeper, or sets another value is still custom. The marker key
itself is ignored at runtime: opencode's config normalizer only copies
known top-level keys into the effective config.

Rollout status is reported as `ok`/`stale`/`missing`/`custom` per repository
plus a summary line; dirty working trees are `blocked` in sync mode and
annotated ` (dirty)` in read-only `--check` mode. Dirty repositories are
coordination-blocked and reported separately — never silently dropped.
The scan uses `git status --porcelain --untracked-files=all`, so the
untracked configs from a previous rollout are visible as themselves
rather than as a collapsed `?? .opencode/`; only foreign files count as
dirt, and the config path plus its `harbor-opencode` write-temps are
allowlisted. Writes are atomic (same-directory temp plus rename), and a
`git status` that fails is treated as foreign dirt — fail-closed, never
as a clean tree.
