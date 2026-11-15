# Harbor vision

Harbor is the superproject for the owned build-infrastructure components:
toolchains, language frameworks, compilers, package sets, and the modules that
install them. Most components are git submodules of their own live repositories,
while `macos-sdk` stays inline because its pin repository is private. Consumers
pin Harbor when they want a consistent, jointly evaluated build environment; the
components keep their own package identities, versions, publication settings, and
licenses.

## Problem

Build infrastructure tends to fragment into many small repositories, each with
its own nixpkgs pin, Rust toolchain, and packaging conventions. Consumers then
either pin each one separately and discover mismatches at build time, or copy the
pieces into one repository and lose independent release history. Both outcomes
make a reproducible toolchain harder than the code it builds.

## Principles

- Independence inside one evaluation. Components retain their own repositories,
  package identities, versions, publication settings, licenses, and imported
  histories; Harbor qualifies them together without merging them into one
  package.
- The root owns shared inputs. Harbor supplies every component input from the
  root flake; a component that adds an input must declare it there, and
  `checks.<system>.component-core` fails if a component depends on something the
  root does not supply.
- Pins stay aligned. Component lock pins match the root pins (nixpkgs,
  nixpkgs-darwin, rust-overlay, flake-parts, and the rest), and
  `component-core`'s `lock-alignment` check enforces it.
- Components also evaluate standalone, reading `harborRoot` when Harbor passes
  it and falling back to harbor-meta's `lib.standaloneRoot`, so a component can
  be developed and tested in its own checkout.
- Qualify every gitlink bump. Change a component in its repository, merge it
  there, then move the gitlink here; Harbor CI runs `component-core` for each
  bump.
- Move the whole stack deliberately: picking a new root nixpkgs means choosing
  the newest revision whose cache-pin requirements are cached, aligning every
  component lock with `scripts/align-component-locks`, landing each component
  change, and only then bumping the gitlinks.
- One owner per tool. The tooling catalog declares each tool's provider once;
  presets resolve packages lazily, and conflicting executable providers require
  an explicit selection or an identical override.
- Nix consumers must fetch submodules, because the `github:` tarball fetcher
  omits them.

## Scope

The root flake and its shared inputs, the gitlinked components (core, rust,
python, javascript, go, android, database, evm, solana, ntt, llm, projects, cad,
tex, game) plus the inline `macos-sdk`, the Cargo workspace membership of the
root, the Simit-generated monorepo CI in `simit.toml`, the `programs.harbor`
framework presets for Home Manager and NixOS, and the migration receipts that
keep pre-conversion history reachable.

## Non-goals

Harbor is not a single monolithic package, does not absorb every specialist
engine it touches, and does not take over a component's own release. It also does
not redistribute the license-restricted macOS SDK; that stays behind the private
pin and cache.

## Direction

The visible direction is continued conversion of the remaining inline component
and steady qualification of component bumps: the most recent changes move the
root nixpkgs for a compiler fix and qualify harbor-cad against stock ParaView,
with the monorepo component structure landing just before them.

## Place in the fleet

Canix pins Harbor as a flake input (its `components/core` direnv package is used
directly), depends on sibling components such as `harbor-rs`, `harbor-db`,
`harbor-game`, and `harbor-macos-sdk-pin`, and paperclip's graphics organisation
manages several of the Harbor repositories as projects.
