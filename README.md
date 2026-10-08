# Harbor

Harbor brings the owned Harbor build-infrastructure repositories into one
history-preserving repository. Components retain independent package identities,
versions, publication settings, and licenses.

## Shared infrastructure

The root `flake.nix` and `flake.lock` own the input graph. Component output
adapters are imported with those inputs and a component-local `self`; their
former input declarations and lockfiles do not participate in root evaluation.
The root Cargo workspace has eight members, with one lockfile and package-scoped
Nix builds. Templates and test fixtures keep their independent project manifests.

| Component                                               | Nix library namespace             |
| ------------------------------------------------------- | --------------------------------- |
| Core conventions and OpenCode policy                    | `lib.core`                        |
| Rust toolchains, packaging, caches, and SDK integration | `lib.rust`                        |
| Python                                                  | `lib.python`                      |
| JavaScript                                              | `lib.javascript`                  |
| Go                                                      | `lib.go`                          |
| Android                                                 | `lib.android`                     |
| TeX and article plugins                                 | `lib.tex`                         |
| Documentation projects                                  | `lib.docs`                        |
| Database lifecycle                                      | `lib.database`                    |
| CAD workers                                             | `lib.cad`                         |
| LLM environments                                        | `lib.llm`                         |
| Blockchain helpers                                      | `lib.blockchain.{evm,solana,ntt}` |
| macOS SDK deployment pin                                | `lib.pins.macosSdk`               |

Packages, checks, apps, development shells, templates, NixOS modules, Home Manager
modules, and formatter modules use `<component>-<name>` output names. For example,
`packages.<system>.rust-harbor-rs`, `nixosModules.database-harbor-db`, and
`homeManagerModules.cad-default`. `default` and `ci` shells share the root
toolchain and formatter; the CI shell also supplies the pinned Simit generator.

## Qualification

```console
simit monorepo plan --json
simit monorepo plan --changed-path components/python/lib/default.nix --json
simit init ci --check --diff
treefmt --ci
simit test --git-fixtures -- cargo test --locked --workspace --jobs 2 -- --test-threads=2
cargo clippy --locked --workspace --all-targets --jobs 2 -- -D warnings
python3 scripts/check_history.py
```

`simit.toml` owns component paths, Cargo packages, dependency edges, native runner
selection, and qualification commands. `checks.<system>.component-<component>`
aggregates every check exported by that component. Database qualification retains
the hosted KVM and sandbox-cache prerequisites. Go keeps its Linux, ARM Linux,
and ARM macOS native runners. Rust qualifies its workspace and Nix outputs on
x86-64 Linux and ARM macOS; namespace runtime acceptance runs on Linux.
Failed and cancelled selected jobs fail the
aggregate qualification gate.

Shared shell constructors accept `timeZone = "Europe/Istanbul"` and default to
UTC. `lib.core.timezone` supplies packaged zone data and an entry hook that
restores the selected zone when Nix omits `TZ` from the shell environment.
Shell composition retains explicit `TZDIR` overrides and interactive Bash.
Release archives accept `archiveTimezone`; portable binary releases also retain
named `extraFiles` with declared modes. Archive checks cover path collisions,
filenames containing spaces, and reproducible tar/ZIP bytes.

## Independent releases

```console
simit release plan --component rust --json
simit release patch --component rust --package harbor-cache -m 'release harbor-cache'
```

Cargo releases use `<package>/v<version>` signed tags and an adjacent package
changelog. Generated publication workflows qualify the full component graph,
verify the trusted tag and exact source revision, and publish only that package.
Dependencies must already be available in the registry. `publish = false` packages
remain excluded. Package license metadata comes from the imported component;
Harbor has no blanket license covering all components.

Static Python and private npm version owners are explicit `releases` entries:
`anx-plugin-pandoc`, `harbor-cad-mcp`, and `harbor-llm`. They support package-scoped
planning, version bumps, adjacent changelogs, verification, and tag synchronization.
Select `--package` for components that also own Cargo members. These packages keep
their existing local-only publication policy; npm's `private` identity is preserved.

Intel macOS outputs use the shared, maintained `nixpkgs-darwin` input. Solana's
locked sources track the selected CLI: Agave 3.0.12 on Intel macOS and 4.0.3 on
the other supported platforms. Evaluation requires no derivation-source reads.

## Imported histories

`migration/sources.json` records all 22 imports, selected source revisions,
original source trees, original remotes, and retained branch namespaces. Each
selected source commit is an ancestor of the monorepository history. Seven older
repository identities remain under `migration/legacy/` with byte-identical source
trees, including their licenses. Pending source edits remain in the original
checkouts, as recorded in the receipts.

Branch tips are published as immutable
`imported-ref/<source>/<local|remote>/heads/<branch>` archival tags. Original tag
objects retain their identities under `imported/` and `imported-local/`.
`migration/refs.json` binds all 164 retained refs to their original object IDs;
`scripts/check_history.py` verifies that receipt on a full clone with tags.

Legacy reconciliation, retained feature branches, binary/site release policy,
and consumer cutover require their own qualification. Existing source checkouts
remain the authority for those pending changes until reconciliation and hosted
acceptance are complete.

See [source reconciliation](migration/reconciliation.md) for absorbed legacy
fixes, incorporated compatibility repairs, and the source-checkout retirement
gate.
