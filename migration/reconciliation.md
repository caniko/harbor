# Source reconciliation

The import boundary is the selected revision in `sources.json`. Retained local
and remote branch tips remain reachable through the archival tags bound by
`refs.json`. These receipts preserve committed work beyond the selected revision;
they do not assert that every retained feature is active in the root outputs.

## Absorbed legacy fixes

`git cherry <successor-revision> <legacy-revision>` establishes patch equivalence
for these non-merge commits at the import boundary:

| Legacy source             | Successor              | Already absorbed commits                                         |
| ------------------------- | ---------------------- | ---------------------------------------------------------------- |
| `db-harbor`               | `harbor-db`            | `f2f9527`, `6d89f45`, `97d11a1`, `a05eba3`, `29ee1bf`, `5f76068` |
| `rs-harbor-macos-sdk-pin` | `harbor-macos-sdk-pin` | `13d4cee`, `ee425ca`                                             |
| `py-harbor`               | `harbor-py`            | `f2ebdc0`, `9a6231b`                                             |

Python commit `7f381ef6aee6a23026a923e947896744fa352e8f` is semantically
absorbed: `components/python/checks/default.nix` still verifies that the shell
without dependency selections contains neither `--extra` nor `--group`, using
`passthru.devShellSpec.shellHook` after the shared-shell refactor.

## Compatibility repairs in the integration tree

The active components incorporate the retained Intel macOS package-set adapters,
Android platform policy, locked Agave source/CLI parity, build-time database
artifact inspection, path-valued NixOS fixture imports, and portable TeX template
evaluation. The shared Darwin input is pinned to
`2bd3427b41d10b8318383195efe502ed1baca6cd`. Root evaluation uses the shared input
graph, rather than each component's historical lockfile.

The root CI shell follows Simit's packaged native platforms: x86-64 Linux,
ARM Linux, and ARM macOS. The default development shell and applicable component
outputs also retain Intel macOS support.

Retained TeX commit `d66c4221d5b14cecd2ae90f1faa9f29bb78fc72b` is absorbed:
the template and its check forward the Intel macOS package set, while the
template formatter and `treefmtModules.tex-latex` use TeX Live's wrapped
`latexindent`, including its Perl runtime dependencies.

The shared-timezone core through `364821e34a81eb3e1f0cd1c6ae37a766f30a08ff`
and interactive-Bash fix `38131b61ee8fd8bc4320b7611c11de8a01a5b578` are
incorporated with their adapter and runtime regression checks. Named-zone shell
forwarding is incorporated for Rust (including MSVC and Android adapters), Python,
JavaScript, Go, Android, TeX, EVM, Solana, NTT, and documentation projects.
`core-timezone-consumers` checks those constructors directly in the relocated
component layout. Archive-timezone and portable supporting-file changes through
`d6108f8f513e0f442c87c20e5ef49ca78f1b550f` are incorporated with the retained
mode, path-collision, whitespace-filename, and byte-reproducibility checks in
`components/rust/checks/release-archives.nix`.

Rust sandbox commits `41d6bad9ef12f3f3043009a07744bee41225e2ce` and
`5f951111c5cb5e26725ad9c038854866807e9cc2` are incorporated into the shared
workspace with their runtime, nested-desktop, CLI, and Nix contract checks.
The CLI integration is Linux-scoped to retain non-Linux release builds, and
the package retains its independent `0.1.0` version and original dual license.
Retained sandbox repair `c736596031ad21ccc29d43c253f214da8060295c` also
qualifies the packaged launcher against a Harbor shell's passthru contract,
supervises private services/compositors, and retries readiness failures without
requiring a source edit. All eight explicitly provisioned Linux runtime cases
passed locally after relocation.

Core hook repair `7e53f32963efa3dd9ce492589f8a948ab2154db0` and Rust hook
assertions `a614bf6a61201c4f3a0b434fcd245c86c30cfe4e` retain cache-independent
`treefmt --ci` qualification. The pending OpenCode formatter-disable change is
incorporated in the shared config producer and its existing policy contract.

Python formatter commits `736e95d1e4c7f5e9edd211eea30ad5533d49ea7d` and
`e439a521f8f99b2ee569bfa4213cda9553c4ed3f` retain the public Python treefmt
module, packaged formatter, default-shell tool, and shared root formatting gate.
Pending unused-argument cleanup is retained in the formatter modules.

PostgreSQL policy commit `532135bb08b69aafa75f5bd8588c03191d041c97` is
incorporated in `components/database/python/harbor_db/writer_fence.py` with its
five retained policy regressions. The relocated database Python suite passes
all 111 cases, including strict physical-replication rules, durable cluster/policy
bindings, and redirected-state rejection.

LLM admission commits `c9cbd2803e5cd69c1945bbf10ddf2efb9648fed7`,
`0d59fabf54e43b84847509e4dd8ae496e9349608`, and
`e947aef908a7ab017f622fa4dfce855213b43975` are incorporated under
`components/llm`. The ESM library and TypeScript declarations are byte-identical
to the final retained source; Python files are AST-identical and the contracts,
metadata, and npm lock are structurally identical after root formatting.
Both distributions retain the private `harbor-llm` identity and `0.1.0` version;
the independent root release namespaces are `harbor-llm` and
`harbor-llm-python`. Source installation now selects the monorepository
component. The license, packed exports, independent authenticated loopback
consumer, cross-language vectors, and declaration checks remain component-owned.

Python `029642dee0dc9e9df9dab2450a403d0148fed052`, TeX
`77ec8d69122253d4c7532c0808742aa8577dfa58`, and Rust
`25da16e36a71b9a3bb23e028d17aca9df13b8197` retain template-module
evaluation and the flake-input/missing-token Attic regressions. The template
checks evaluate shared language modules rather than matching their source text.

## Retained-branch dispositions

[Retained work](retained-work.md) records every one of the 112 distinct commits
reachable from the receipt-bound archival head tips but outside each source's
selected import revision. It includes branch tips beyond the original checkout
HEAD, not just its committed-delta snapshot. Tips already ancestral to the
selected revision are absorbed by that source import. A preserved ref or an
incorporated implementation is not itself a hosted acceptance receipt.

## Pending-patch dispositions

The 2026-10-09 refresh revalidated HEAD, tracked patches, status, and untracked
bytes for all 22 original checkouts. No source HEAD or tracked patch changed
since the original snapshot. Core gained only five `.nix-results/` store-output
symlinks; the refreshed receipt records their targets without copying closures.
Its `report.json` SHA-256 is
`a07e4d3585abf1c008585b3c68fa583b5b09b60f728d9e5e40efc5ea10c3b03c` at
`/data/scratch/tmp/opencode/harbor-source-reconciliation-20261009-v2`.

The following dispositions apply to those exact snapshots. Committed feature
work is incorporated as described above. Former source-specific pins, standalone
workflow layouts, local `.envrc` overrides, and workflow-concurrency edits are
superseded by the root input graph, approved root environment, and generated
component/package workflows. The retained archive refs still preserve their
original commits.

| Original checkout         | Active owner | Pending source disposition                                                                                                                                                                                                                                                                                                         |
| ------------------------- | ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `db-harbor`               | database     | Module exports retain the old identity through the database alias. Pending Nix changes only alter formatting and `inherit` spelling; the active module/evaluation code supersedes that spelling.                                                                                                                                   |
| `harbor-android`          | Android      | Public Java/Kotlin modules are retained. Pending shell/Nix reformatting, `inherit` spelling, and unused lambda renames are superseded by the active root formatter policy.                                                                                                                                                         |
| `harbor-cad`              | CAD          | No pending patch or untracked source at the import boundary.                                                                                                                                                                                                                                                                       |
| `harbor-db`               | database     | Cargo lock-entry reordering is superseded by the shared lock. Pending module `inherit` spelling and unused argument renames do not change lifecycle commands or dependencies.                                                                                                                                                      |
| `harbor-eth`              | EVM          | The untracked local formatter entrypoint is superseded by root formatting and the retained public Solidity module.                                                                                                                                                                                                                 |
| `harbor-go`               | Go           | No pending patch or untracked source.                                                                                                                                                                                                                                                                                              |
| `harbor-js`               | JavaScript   | The public formatter retains the language patterns. Pending `inherit` spelling and the local formatter entrypoint are superseded by active library/template code and root formatting.                                                                                                                                              |
| `harbor-llm`              | LLM          | The untracked Nix-only formatter entrypoint is superseded by the root Nix formatter.                                                                                                                                                                                                                                               |
| `harbor-macos-sdk-pin`    | macOS SDK    | No pending patch or untracked source. The SDK data remains independently owned under `lib.pins.macosSdk`.                                                                                                                                                                                                                          |
| `harbor-meta`             | core         | Artifact acceptance is incorporated in `lib/artifact-acceptance.*`, `lib/package-tests.nix`, its check, tests, and docs. OpenCode formatter disabling and `--ci` hooks are incorporated. Remaining `inherit`/unused-binding/style edits are superseded by active code. Python bytecode and store-output links are generated state. |
| `harbor-ntt`              | NTT          | The untracked Nix/TOML formatter entrypoint is superseded by the root programs.                                                                                                                                                                                                                                                    |
| `harbor-projects`         | docs         | Intel macOS package-set selection is incorporated. Pending `inherit` spelling and site-helper argument cleanup are superseded by the active constructors; standalone CI and formatter files are superseded by the root owners.                                                                                                     |
| `harbor-py`               | Python       | Public formatter modules and default-shell tools are incorporated. Pending FFmpeg fallback shorthand, unused lambda names, and removal of unused wrapper bindings are spelling/dead-binding cleanup; active behavior is retained.                                                                                                  |
| `harbor-rs`               | Rust         | Pending changes are `inherit`/fallback spelling, unused parameters/bindings, and template/site formatting. The active release, build-cache, service, and packaging helpers retain behavior; sandbox and portable-release features are incorporated separately.                                                                     |
| `harbor-sol`              | Solana       | Pending output-library argument cleanup is superseded by the active constructor. Locked source/CLI parity and Intel macOS behavior are incorporated.                                                                                                                                                                               |
| `harbor-tex`              | TeX          | Pending helper `inherit` spelling, Rust import ordering, and template formatting are superseded by active code and its formatting policy. Wrapped TeX formatter exports are incorporated.                                                                                                                                          |
| `js-harbor`               | JavaScript   | Legacy workflow and Nix argument/style edits are superseded by the active JavaScript implementation and root workflow/input owners.                                                                                                                                                                                                |
| `meta-harbor`             | core         | Legacy workflow, `inherit` spelling, and unused-binding/argument removal are superseded by the active core implementation.                                                                                                                                                                                                         |
| `py-harbor`               | Python       | Legacy formatting, FFmpeg shorthand, and unused wrapper-binding removal are superseded by the active Python implementation. The no-selection shell regression remains covered.                                                                                                                                                     |
| `rs-harbor`               | Rust         | Legacy Crow-to-GitHub/per-member workflow work is superseded by generated component/package CI. Pending SDK formatting, `inherit` spelling, parenthesis cleanup, and unused helper bindings/parameters are superseded by active Rust/Android owners.                                                                               |
| `rs-harbor-macos-sdk-pin` | macOS SDK    | Pending workflow-concurrency spelling is superseded by root CI.                                                                                                                                                                                                                                                                    |
| `tex-harbor`              | TeX          | Legacy workflow, Nix spelling/unused-helper arguments, and Rust import ordering are superseded by active TeX code.                                                                                                                                                                                                                 |

The untracked formatter entrypoints in the snapshots are repository-local
composition files, not additional language engines or version owners. Root
formatting deliberately excludes imported templates and legacy receipts; their
public language modules and template contracts remain component-owned. Dead
binding removal is not required to recover a feature, so it does not justify
rewriting otherwise unchanged imported helper files during integration.

## Preserved work and retirement gate

The 2026-10-08 source snapshot contains each original checkout's HEAD/status,
committed delta, pending patch, and untracked file bytes. Its `report.json` SHA-256
is `36f2aaef83d106a141a1544c6cb37477f08b608ad2104ff06c435fb374fdc65d`.
The operator evidence directory is
`/data/scratch/tmp/opencode/harbor-source-reconciliation-20261008`.

Retained work includes the named-timezone shell feature across components,
Rust sandbox and portable-release feature work, formatter-module updates, and
pending source-checkout edits. Original checkouts remain authoritative for that
pending work. Before retiring an old identity, refresh its snapshot, reconcile
each retained branch and pending patch into an accepted component revision or
record its explicit supersession, qualify the resulting component, and verify
every consumer against the accepted monorepository revision.

Harbor hosted qualification precedes consumer cutover. A passing import-history
check or an evaluation-only receipt establishes only its stated scope.
