# Harbor

Harbor is the superproject for the owned Harbor build-infrastructure
repositories. Most components are git submodules of their own live repositories
(for example `components/rust` is `caniko/harbor-rs`); `cad`, `tex` and
`macos-sdk` are still inline. Components retain independent package identities,
versions, publication settings, and licenses, and the imported histories remain
reachable here.

## Working with submodules

```console
git clone --recurse-submodules https://github.com/caniko/harbor.git
git submodule update --init --recursive   # existing checkout
```

Change a component in its own repository (or in `components/<name>`, which is
that repository's checkout), merge it there, then bump the gitlink here:
`git -C components/<name> fetch && git -C components/<name> checkout <rev>`
and commit `components/<name>`. Harbor CI qualifies every bump. Each component
also evaluates standalone: it reads `harborRoot` when harbor passes it and
otherwise falls back to harbor-meta's `lib.standaloneRoot`. When a component
adds a flake input, declare it in the root `flake.nix` too (the root supplies
every component input; `checks.<system>.component-core` fails otherwise).
Component lock pins
match the root pins (nixpkgs, nixpkgs-darwin, rust-overlay, flake-parts, ...);
keep them aligned when the root moves. Nix consumers must fetch submodules:
`git+https://github.com/caniko/harbor.git?ref=...&submodules=1`; the
`github:` tarball fetcher omits them.

## Shared infrastructure

Framework installers are available through `homeManagerModules.default` and
`nixosModules.default`. Select full presets with `programs.harbor.frameworks`,
override individual packages, and opt into client delivery through the Fleetix
adapter. See [framework tooling modules](docs/tooling-modules.md) for the exact
catalog, platform support, Blender/OpenPencil MCP and CAD runtime contracts.

The root `flake.nix` and `flake.lock` own the input graph. Component output
adapters are imported with those inputs and a component-local `self`; their own
input declarations and lockfiles serve standalone evaluation only. The root
Cargo workspace holds core's `harbor-evidence`, database, CAD and the Zenodo
plugin with one lockfile; `components/rust` and `components/llm` are their own
Cargo workspaces with their own lockfiles. Templates and test fixtures keep
their independent project manifests.

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
On Linux, both shells consume the database check's PostgreSQL and immutable
configuration fixtures for native workspace tests. Native tests retain the
installed machine identity; sandboxed DB checks own their testing-only identity
substitution.

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
x86-64 Linux and ARM macOS; namespace runtime acceptance runs on Linux. Linux
qualifies all nine Cargo members. macOS qualifies the seven portable members
with `--workspace --exclude harbor-cad --exclude harbor-db`: CAD's worker and
DB's lifecycle engines are Linux-only and retain their dedicated Linux component
and runtime checks.
ELF audit contracts use a deterministic ELF fixture on every native platform;
Linux also audits the cargo-built executable, and resolver tests check both
`--skip-ldd` isolation and the missing-library failure path.
Resolver fixtures use the build environment's shell, including in Nix sandboxes.
All thirteen project templates consume one revision-qualified Harbor input and its
namespaced libraries and formatter modules. `checks.<system>.template-contract`
evaluates their native Linux packages, shells and formatters; the core aggregate
includes that gate. Template lockfiles preserve their own project input graphs,
and the Rust hook checks also evaluate those pinned graphs independently.
Python development shells advertise the four maintained Harbor systems,
including Intel macOS through `nixpkgs-darwin`; experimental nixpkgs platforms
are not included in the Python shell inventory.
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
