# harbor-meta

Shared harbor helpers for editor and agent tooling.

`harbor-meta` owns cross-language policy shared by the language harbors.

Formatting uses independent [treefmt modules](docs/treefmt.md), composed
explicitly by each repository rather than inherited from a parent Harbor.

Dev shells are fragments `{ packages, env, shellHook }`. Language harbors merge
those fragments and render them with `lib.devShell.mkShell` (`pkgs.mkShell` by
default; Rust passes a `craneLib.devShell` builder). `lib.devShellTests` and
`lib.templateTests` evaluate harbor templates by importing their `flake.nix`
with the current flake inputs — no nested `nix` and no template lockfiles.

`lib.flake.mkDerivationManifest` records exact derivation paths and selected
outputs without retaining string context. Orchestrators can serialize that
manifest and realize the same derivations locally or on an explicitly selected
remote builder without changing store identity.

When a profile registry is bound, checked-in `.opencode/opencode.jsonc`
files declare the LSPs opencode may use, while each project's
direnv-powered harbor dev shell supplies the actual binaries.

OpenPencil MCP is opt-in: default generated configs carry no `mcp` block.
Pass `--openpencil` to register the server. The desktop binary must already
be on PATH. OpenCode code mode (`OPENCODE_EXPERIMENTAL_CODE_MODE`) defers
MCP schemas behind the `execute` tool.

## Agent shells and validation

For projects using `.envrc` with `use flake .`, agent subprocesses do not
necessarily load direnv automatically. Run `direnv exec . <command>` from the
project root for each invocation; loading one subprocess does not change its
parent's PATH. If `.envrc` is blocked, stop for review and explicit authorization
before `direnv allow .`. Never silently authorize a project's shell hooks.

Use `lib.devShellTests.mkCheck` with an explicit `commands` list to check that
tools are available from the shell's native build inputs without the user's
host PATH. Build the resulting check: `nix flake check --no-build` only evaluates
it. This is an executable-availability check, not a runtime or direnv activation
test. Keep application startup and dependency-import checks separate.

## Usage

```bash
harbor-opencode sync --kind detect
harbor-opencode sync --kind detect --openpencil
harbor-opencode check --kind detect
harbor-opencode rollout --root /data/nvme0/can/Projects
```

The `harbor-opencode` built by this repository deliberately binds no
profile registry: `detect` and `none` both render the policy-only config
(the format-permission deny fragment without an `lsp` block), and any
profile name is rejected as unsupported. Language-aware builds bind a
registry in their own flake through `lib.opencode.mkCli { inherit pkgs;
profiles = ...; }` — harbor-rs ships the `rust` profile and the
corresponding binary.
