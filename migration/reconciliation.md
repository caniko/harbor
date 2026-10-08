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
