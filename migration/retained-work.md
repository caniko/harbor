# Retained branch work

This disposition record is bound to the 22 import boundaries in `sources.json`
and the 164 archival refs in `refs.json` (mapping SHA-256
`2311d4bf813d2ed1dbd7547a178b73d3789743df40064633b7ae4e7e536c00e3`).
It covers the union of `git rev-list <archival-head-tip> ^<sourceRevision>` for
each source, including merge commits. The union contains 112 distinct commits;
a shared historical commit may appear under more than one original identity.
Eight-character IDs below resolve to the preserved full Git objects. Tips whose
commit set is empty are already ancestral to the selected import boundary.

**Incorporated** means that the active component contains the feature or an
equivalent implementation and its retained contract. **Absorbed** means that
the selected import already contains equivalent work or the commit only merges
the separately recorded work. **Superseded** names the authoritative replacement;
it does not delete the original archival ref or change its historical source.
Qualification and consumer acceptance remain the separate retirement gate in
`reconciliation.md`.

## db-harbor

| Commits                | Disposition  | Active evidence / replacement                                                                         |
| ---------------------- | ------------ | ----------------------------------------------------------------------------------------------------- |
| `8f4a3da2`             | Incorporated | `components/database/flake.nix` retains `nixosModules.db-harbor` alongside the newer module identity. |
| `359d832b`, `fc66bfd9` | Superseded   | Root `flake.nix`/`flake.lock` own canonical sources and the shared Rust pin.                          |

## harbor-android

| Commits                | Disposition  | Active evidence / replacement                                                                                                                              |
| ---------------------- | ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `350c791c`, `4f930da3` | Incorporated | `components/android/checks/platform-policy.nix` isolates fixture HOME and checks the shared Intel macOS tooling adapter.                                   |
| `f4830e5b`, `9f6d0db5` | Incorporated | `components/android/lib/android-dev-shell.nix`, core timezone adapters and shell-entry checks retain named zones. Root pins replace source-specific locks. |
| `3682b91f`             | Superseded   | Root Simit component CI and generated workflows replace the standalone workflow.                                                                           |

## harbor-cad

No retained head-tip commits lie outside the selected clean local import.

## harbor-db

| Commits                | Disposition  | Active evidence / replacement                                                                                                                                                                                                                                                                                                                                                                   |
| ---------------------- | ------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `17024c91`, `2c2a18cb` | Incorporated | `components/database/nix/module-eval-runtime.py`, `module-eval.nix`, and `pg-backup-eval.nix` retain build-time artifact inspection and path-valued fixture imports.                                                                                                                                                                                                                            |
| `532135bb`             | Incorporated | `components/database/python/harbor_db/writer_fence.py` and `tests/test_writer_fence.py` retain explicit PostgreSQL writer policy and five regressions.                                                                                                                                                                                                                                          |
| `1630defa`, `3921840f` | Incorporated | Database shell construction uses core named-zone policy and its shell-entry repair.                                                                                                                                                                                                                                                                                                             |
| `14b7ac1a`, `c4400952` | Superseded   | Root qualification realizes the database aggregate/lifecycle checks on the exact candidate. Original PR-specific provider-readback scripts, larger-runner admission, and evidence-retention workflow are preserved historically; the root workflow does not claim those separate provider-retention guarantees. Root read-only workflow permissions replace the standalone privileged PR token. |
| `8391efd3`, `ae0917c3` | Superseded   | Root component CI declares hosted database cache prerequisites explicitly in `simit.toml`; root inputs and Canix-scoped private closure publication replace standalone pin/prebuild wiring.                                                                                                                                                                                                     |

## harbor-eth

| Commits                | Disposition  | Active evidence / replacement                                                                        |
| ---------------------- | ------------ | ---------------------------------------------------------------------------------------------------- |
| `012464c5`             | Incorporated | `components/evm/flake.nix` and its template/check retain Intel macOS Foundry tooling.                |
| `e3f011a3`, `d7134008` | Incorporated | `components/evm/lib/default.nix` forwards named-zone policy to the core shell constructor.           |
| `2e57592a`             | Superseded   | The active constructor/root adapter owns argument binding; unused-parameter cleanup adds no feature. |
| `cc2f982b`             | Superseded   | Root generated component workflow.                                                                   |

## harbor-go

| Commits                | Disposition  | Active evidence / replacement                                                                                 |
| ---------------------- | ------------ | ------------------------------------------------------------------------------------------------------------- |
| `28aa896b`, `d6761d30` | Incorporated | `components/go/lib/default.nix` and core timezone consumer checks retain named-zone and shell-entry behavior. |

## harbor-js

| Commits                | Disposition  | Active evidence / replacement                                                             |
| ---------------------- | ------------ | ----------------------------------------------------------------------------------------- |
| `531f3e5e`             | Incorporated | `components/javascript/flake.nix`, checks, and both templates retain Intel macOS tooling. |
| `9dea343d`, `da6bd28b` | Incorporated | Node/Bun constructors retain core named-zone policy and the shell-entry repair.           |

## harbor-llm

| Commits                            | Disposition  | Active evidence / replacement                                                                                                                                                                                       |
| ---------------------------------- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `c9cbd280`, `0d59fabf`, `e947aef9` | Incorporated | `components/llm/src/mcp-admission*`, `python/harbor_llm`, canonical `contracts/`, and retained TypeScript/packed-export/loopback/Python conformance tests. Both private distribution identities remain independent. |

## harbor-macos-sdk-pin

| Commits                                        | Disposition | Active evidence / replacement                                                                                                                    |
| ---------------------------------------------- | ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `13d4cee8`, `ee425cad`                         | Absorbed    | These commits are patch-equivalent at the legacy/successor import boundary; root CI now owns generation.                                         |
| `031f9b44`, `53b11230`, `74109687`, `c95a4738` | Superseded  | Canonical monorepository identity in root README/flake; SDK data remains `lib.pins.macosSdk`. Historical source names remain in import receipts. |
| `2e552ba6`                                     | Superseded  | Approved root `.envrc` replaces the former checkout's local environment.                                                                         |
| `458e5c61`, `c2ebdcfe`, `d1e7c167`             | Superseded  | Root generated SDK component CI.                                                                                                                 |

## harbor-meta

| Commits                                                                | Disposition  | Active evidence / replacement                                                                                                                                                                           |
| ---------------------------------------------------------------------- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `9ca49ab5`, `00edf33a`, `44a7c7cb`, `4b94a2a9`, `1f272a44`, `364821e3` | Incorporated | `components/core/lib/timezone.nix`, `lib/shell.nix`, timezone tests, and relocated cross-component consumer checks retain metadata/export style/custom databases and effective shell-entry restoration. |
| `38131b61`, `30e87aa0`                                                 | Incorporated | Interactive Bash and native setup regression in core shell/check/test files.                                                                                                                            |
| `7e53f329`                                                             | Incorporated | `components/core/lib/hooks.nix` uses uncached `treefmt --ci`.                                                                                                                                           |
| `2bcbded9`, `c7ba4380`                                                 | Absorbed     | Merge ancestry combines the separately incorporated timezone/Bash work.                                                                                                                                 |
| `c86dd5f7`, `eb4c6f86`                                                 | Superseded   | Root generated CI replaces component-local CI/Pages serialization.                                                                                                                                      |

## harbor-ntt

| Commits                | Disposition  | Active evidence / replacement                      |
| ---------------------- | ------------ | -------------------------------------------------- |
| `55ec619c`, `83a7103b` | Incorporated | NTT constructor and core timezone consumer checks. |
| `d0993510`             | Superseded   | Root generated component workflow.                 |

## harbor-projects

| Commits    | Disposition  | Active evidence / replacement                                                                          |
| ---------- | ------------ | ------------------------------------------------------------------------------------------------------ |
| `9dab8e7a` | Incorporated | `components/projects/lib/default.nix` and `lib/docs.nix` retain named-zone documentation shell policy. |

## harbor-py

| Commits                            | Disposition  | Active evidence / replacement                                                                                                |
| ---------------------------------- | ------------ | ---------------------------------------------------------------------------------------------------------------------------- |
| `029642de`, `736e95d1`, `e439a521` | Incorporated | Public Python language module, template evaluation assertions, and packaged/shared formatting gate in `components/python`.   |
| `7f381ef6`                         | Absorbed     | Existing `uv-dev-shell` check verifies no-selection `--extra`/`--group` exclusion through `passthru.devShellSpec.shellHook`. |
| `ca2eed83`, `d8d116eb`             | Incorporated | Python shell constructor forwards core timezone policy and shell-entry restoration.                                          |
| `cd57d6b1`                         | Incorporated | Python package-set adapter and template/check retain Intel macOS support.                                                    |
| `9a6231b8`, `f2ebdc05`             | Absorbed     | Patch-equivalent legacy work at the selected successor boundary; source pins are now root-owned.                             |
| `8fd50b6a`                         | Absorbed     | Merge ancestry, with changes accounted for separately.                                                                       |
| `01bcb140`, `cb7e8332`             | Superseded   | Root approved environment and shared input graph replace standalone environment/lock wiring.                                 |
| `38723d59`, `b5802239`, `feadea90` | Superseded   | Root generated Python component CI.                                                                                          |

## harbor-rs

| Commits                            | Disposition  | Active evidence / replacement                                                                                                                                                            |
| ---------------------------------- | ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `0997cc64`, `195f8a7e`             | Incorporated | Portable/release/archive helpers retain supporting files, declared modes, and shared named-zone wiring, with release-archive regressions.                                                |
| `41d6bad9`, `c7365960`             | Incorporated | Shared workspace `harbor-sandbox`, Linux CLI, packaged launcher/passthru check, readiness retries, and service/compositor supervision.                                                   |
| `5f951111`                         | Incorporated | Runtime and exact-output qualification plus package-specific sandbox publication are represented by root `simit.toml` and generated workflows. Standalone workflow layout is superseded. |
| `25da16e3`                         | Incorporated | `components/rust/lib/attic-push.nix` retains recursive input publication and isolated credential configuration; flake-input and missing-token checks remain in `checks.nix`.             |
| `5f6f5eba`, `e5e74218`             | Incorporated | Idempotent readiness and Dioxus cache dispatcher remain in `components/rust/lib/build-cache.nix`.                                                                                        |
| `a614bf6a`                         | Incorporated | Rust checks verify packaged `treefmt --ci` launchers.                                                                                                                                    |
| `40d44a2f`, `8dd739a8`             | Incorporated | Core timezone repair and adaptation are consumed by Rust, MSVC and Android shell constructors.                                                                                           |
| `a403a691`                         | Incorporated | Root input graph uses the public OpenCode LSP source; original component APIs consume its shared binding.                                                                                |
| `0c84aec0`, `e2778ff3`, `ec2a17a4` | Absorbed     | Merge ancestry combines separately accounted-for features/CI.                                                                                                                            |
| `ff2f2d64`                         | Superseded   | Unused-parameter cleanup is covered by active constructor binding/root formatting; no runtime feature is removed.                                                                        |
| `ff7c0d17`                         | Superseded   | Root Cargo has no checked-in unstable `.cargo/config.toml`; environment/toolchain policy is root-shell-owned, so stable Cargo does not inherit the historical nightly configuration.     |

## harbor-sol

| Commits                | Disposition  | Active evidence / replacement                                                                                |
| ---------------------- | ------------ | ------------------------------------------------------------------------------------------------------------ |
| `d16d7094`, `5a72020d` | Incorporated | Locked Agave source/CLI parity, manifest-only evaluation, and Intel macOS single-pass Agave 3 compatibility. |
| `081fd051`, `b9e76199` | Incorporated | Solana shell constructor and core timezone consumer checks.                                                  |
| `5a7b9a05`             | Superseded   | Root generated Solana component CI.                                                                          |

## harbor-tex

| Commits                | Disposition  | Active evidence / replacement                                                                                                 |
| ---------------------- | ------------ | ----------------------------------------------------------------------------------------------------------------------------- |
| `77ec8d69`, `d66c4221` | Incorporated | Template module evaluation and the wrapped TeX Live formatter retain portable runtime dependencies and Intel macOS selection. |
| `7a16caea`, `4b887714` | Incorporated | TeX shell constructor and core timezone consumer checks.                                                                      |
| `a5d61200`             | Superseded   | Active helpers/root bindings retain behavior; unused-argument cleanup is not a separate feature.                              |
| `2f02c2bd`             | Superseded   | Root canonical input graph replaces source/template lock ownership.                                                           |
| `8d999f9f`             | Superseded   | Root generated TeX component CI.                                                                                              |

## js-harbor

No retained head-tip commits lie outside the selected legacy import.

## meta-harbor

| Commits                            | Disposition | Active evidence / replacement                                                    |
| ---------------------------------- | ----------- | -------------------------------------------------------------------------------- |
| `358e6d8a`, `97fa4a0e`, `c3804eeb` | Superseded  | Root generated core component CI replaces legacy GitHub/Forgejo workflow layout. |

## py-harbor

| Commits                | Disposition | Active evidence / replacement                        |
| ---------------------- | ----------- | ---------------------------------------------------- |
| `8babb492`, `c567e1e0` | Superseded  | Root canonical input graph and approved environment. |

## rs-harbor

| Commits                            | Disposition  | Active evidence / replacement                                                                                                      |
| ---------------------------------- | ------------ | ---------------------------------------------------------------------------------------------------------------------------------- |
| `e5e74218`                         | Incorporated | Same retained dispatcher repair as the successor source, active in Rust build-cache policy.                                        |
| `412b8d90`                         | Superseded   | Active root/platform adapters own host predicates; predicate spelling is behavior-equivalent.                                      |
| `1fd62dd7`                         | Superseded   | Root approved environment consumes the pinned public input instead of a local sibling override.                                    |
| `85ced033`, `8b4191fc`, `b8427f0e` | Superseded   | Root canonical input/lock graph replaces historical site and Bevy pins. Public site/Bevy constructors remain in `components/rust`. |

## rs-harbor-macos-sdk-pin

| Commits    | Disposition | Active evidence / replacement |
| ---------- | ----------- | ----------------------------- |
| `a02df1ca` | Superseded  | Root approved `.envrc`.       |

## tex-harbor

No retained head-tip commits lie outside the selected legacy import.
