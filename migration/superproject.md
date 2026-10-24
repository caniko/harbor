# Superproject conversion

On 2026-10-22, twelve components became git submodules of their live
repositories. Before the gitlinks replaced the inline trees, each repository
received harbor's component tree through a 3-way merge against the last
predecessor revision harbor had absorbed (`sources.json`, or the later
absorption recorded in `database-default-refresh.json` and `retained-updates.md`).
Predecessor work made after that base is kept. Examples: the harbor-rs
build-native toolchain and Android NDK linker fixes, the harbor-db Btrfs crash
fixture, the lifecycle fixes and the original-obligation contract, the
harbor-android SDK environment fix, and harbor-llm's version 2 preparation
admission.

| Component  | Repository             | Gitlink   |
| ---------- | ---------------------- | --------- |
| core       | caniko/harbor-meta     | `0fd1008` |
| rust       | caniko/harbor-rs       | `fcc3d75` |
| python     | caniko/harbor-py       | `535cc40` |
| javascript | caniko/harbor-js       | `51cb062` |
| go         | caniko/harbor-go       | `9785b9c` |
| android    | caniko/harbor-android  | `7af2976` |
| database   | caniko/harbor-db       | `d704a10` |
| evm        | caniko/harbor-eth      | `6060c62` |
| solana     | caniko/harbor-sol      | `4ee4fe0` |
| ntt        | caniko/harbor-ntt      | `57d2429` |
| llm        | caniko/harbor-llm      | `4062374` |
| projects   | caniko/harbor-projects | `8e7503d` |

The table records the conversion; later gitlink bumps are in the history.

On 2026-10-24, `cad` and `tex` followed the same recipe and absorbed harbor #6
(registry Fleetix in CAD) along the way:

| Component | Repository        | Gitlink   |
| --------- | ----------------- | --------- |
| cad       | caniko/harbor-cad | `836aee4` |
| tex       | caniko/harbor-tex | `a0f9d1f` |

Both remain root Cargo workspace members (`components/cad`,
`components/tex/plugins/zenodo`). Their crate-local `Cargo.lock` files serve
standalone builds. `macos-sdk` stays inline because
`caniko/harbor-macos-sdk-pin` is private.

Every difference between harbor's pre-conversion tree (`a3aa8ed`) and the gitlinked
revisions is one of three kinds:

- predecessor work listed above;
- standalone-evaluation changes: declared inputs, lock pins aligned with the
  root, `harborRoot` / `harborFormatting` read through `outputArgs` with
  harbor-meta's `lib.standaloneRoot` as the fallback, and component-local
  Cargo lockfiles;
- regenerated component CI.

The cross-component `timezone-consumers` check moved from core's flake to the root
(`checks.<system>.core-timezone-consumers`), since it reads sibling components.
`components/rust` and `components/llm` became separate Cargo workspaces. The root
workspace excludes them, and the root `Cargo.lock` dropped their 41 packages
without changing any remaining version.

Upgrading to Simit trunk applies simit #58's rule that every release owner
needs its own CHANGELOG.md directory. harbor-llm #4 moved the protocol crate's
manifest to `rust/` and the Python package to `python/` (hence gitlink
`4062374`). CAD's MCP package (`pyproject.toml`, `uv.lock`) moved to
`components/cad/python/`.

Imported histories, archival refs and `scripts/check_history.py` are unchanged.
Commit ancestry still holds every import boundary.
