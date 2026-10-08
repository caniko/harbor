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

Core hook repair `7e53f32963efa3dd9ce492589f8a948ab2154db0` and Rust hook
assertions `a614bf6a61201c4f3a0b434fcd245c86c30cfe4e` retain cache-independent
`treefmt --ci` qualification. The pending OpenCode formatter-disable change is
incorporated in the shared config producer and its existing policy contract.

Python formatter commits `736e95d1e4c7f5e9edd211eea30ad5533d49ea7d` and
`e439a521f8f99b2ee569bfa4213cda9553c4ed3f` retain the public Python treefmt
module, packaged formatter, default-shell tool, and shared root formatting gate.
Pending unused-argument cleanup is retained in the formatter modules.

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
