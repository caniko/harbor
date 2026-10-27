# Framework tooling modules

Harbor exports `homeManagerModules.default` and `nixosModules.default` with the
same `programs.harbor` selection interface. Selection is disabled by default.
Enabling it includes core tools and the full curated presets selected below.

```nix
{
  imports = [inputs.harbor.homeManagerModules.default];
  programs.harbor = {
    enable = true;
    frameworks = ["rust" "python" "javascript" "cad" "tex"];
    tools.blender.mcp.autoConnect = false;
    tools.openpencil.desktop.enable = true;
    tools.uv.package = pkgs.uv;
    tools.pyright.enable = false;
  };
}
```

## Presets and ownership

The exact tool-to-framework mapping and supported systems are maintained once
in `nix/tooling/catalog.nix`, exposed as `lib.tooling.catalog`. Packages are
resolved lazily; disabled tools do not require their default packages or pins.
`programs.harbor.resolved` reports selected, unavailable and unsupported tools.
Explicit tool settings override preset defaults. An unsupported required preset
or explicitly enabled tool fails with an assertion; optional platform-specific
helpers default off.

| Selection  | Curated tools                                                                |
| ---------- | ---------------------------------------------------------------------------- |
| core       | Harbor OpenCode policy helper, nixd, taplo, treefmt                          |
| rust       | Pinned Rust/analyzer, sccache executable, available Harbor CLI/CI helpers    |
| python     | Python 3.13, uv, Ruff and Pyright                                            |
| javascript | Node, pnpm, Bun, TypeScript language server and OpenPencil                   |
| go         | Harbor's Go, gopls and golangci-lint builds                                  |
| android    | Explicit SDK/NDK composition, Rust, device helper, cargo-ndk, JDK 21, Gradle |
| tex        | Curated TeX profile, texlab, Pandoc and anx plugins                          |
| projects   | mdBook, Pandoc, Node/pnpm/Bun and OpenPencil                                 |
| database   | Harbor DB and lifecycle executables                                          |
| cad        | CAD CLI/MCP, FreeCAD, Blender/MCP and the CPU runtime manifest               |
| llm        | Packaged Harbor harness integrations and the existing Home Manager options   |
| evm        | Foundry and solc                                                             |
| solana     | Rust, Anchor, Agave and cargo-build-sbf                                      |
| ntt        | Consumer-pinned NTT CLI and its native toolchain                             |
| macos-sdk  | Available SDK validation tooling; existing realization/GC-root options       |

OpenPencil's CLI is portable across the four root systems. Its patched desktop
is currently x86_64 Linux; desktop-backed MCP registration defaults on there.
Blender MCP supports Linux. CAD is x86_64 Linux, and LLM/database support Linux.
Go retains its three established native lanes.

Package installation has one owner per tool. The root module composes existing
CAD, LLM, database, compiler-cache and SDK modules; persistent workers and
infrastructure services retain their explicit enable/configuration options.
Selecting Rust installs sccache without enabling a daemon or changing Nix policy.
Project-specific offline flags, Rustup mutations and shell hooks stay in the
existing project-shell constructors. Conflicting declared executable providers
require an explicit selection or an identical package override.
CAD and LLM package options alias their existing child-module options. OpenPencil
desktop overrides must expose `passthru.cli` and `passthru.upstreamVersion` matching
the selected CLI; the default builder threads the CLI override through its wrapper.

## MCP delivery

Home Manager exposes local declarations under `programs.harbor.mcp.servers`.
Client delivery uses the consumer's Fleetix dependency:

```nix
{
  imports = [
    inputs.harbor.homeManagerModules.default
    inputs.harbor.homeManagerModules.fleetix
    inputs.fleetix.homeModules.mcp
  ];
  programs.harbor = {enable = true; frameworks = ["cad" "javascript"];};
  fleetix.mcp = {
    enable = true;
    harnesses.opencode.enable = true;
  };
}
```

The adapter imports no Fleetix flake and generates no client-specific formats.
Existing client settings remain owned by Fleetix. Other consumers can deliver
the same typed server records through their own integration. The OpenPencil
bridge command is `openpencil-mcp`; its document tools need an open desktop
document. Headless file operations use the CLI directly.

Blender uses matching server/extension versions, a private instance registry and
screenshot transport, and the existing immutable extension/bootstrap. The
`canix` extension repository identity, workspace paths and `CANIX_BLENDER_MCP_*`
variables are retained for existing saved preferences. A user reservation of AI
control is preserved. Consumers choose a GPU-specific Blender through the normal
package override. Installed OpenCode clients receive the retained tool-permission
defaults and skill when the corresponding options exist.

## Skills

`skills` in `nix/tooling/catalog.nix` (`lib.tooling.catalog.skills`) declares
agent skills whose content lives in a component (`owner`, `source`) and whose
runtime is declared as `requires.frameworks`, optionally narrowed by
`requires.tools` (which must belong to those frameworks), plus
`requires.tex.extraPackages` for TeX packages beyond the article profile. The
required tools (default: every tool of those frameworks), filtered to the host
system, are realized into `skills/<name>/env/bin` of the bundle, so a skill runs `env/bin/python3` or `env/bin/latexmk` instead of
relying on the user's `PATH`.

`packages.<system>.llm-skills` bundles every catalog skill with a
`Skillnet.pkl` manifest (`skills/<name>` sources). `checks.<system>.llm-skills`
checks each `SKILL.md` name against its catalog key, runs the dataset-report
reconciler self-test and compiles its report template with the bundled TeX.
`programs.harbor.skills = ["dataset-report"]` builds the same bundle for a
module; `homeManagerModules.skillnet` appends its manifest to
`programs.skillnet.externalManifests` when the consumer provides that option.

## Explicit framework inputs

```nix
programs.harbor.settings.android = {
  platformVersions = ["34"];
  buildToolsVersions = ["34.0.0"];
  ndkVersions = ["29.0.14206865"];
};
programs.harbor.settings.tex.profile = "article";
programs.harbor.settings.rust.profile = "nightly";
programs.harbor.settings.ntt = {
  pinsFile = ./ntt-pins.json;
  rustToolchain = myPinnedAnchorRust;
};
```

Android and NTT inputs are required by their default providers. A complete
package override can supply a separately configured toolchain. TeX profile names
come from the existing library. NTT pin validation/building stays in its existing
constructor; secrets and deployment policy belong to the consumer.

CAD runtime manifests are selected with `tools.cad.backends`, whose enum is
derived from the existing `cad-runtime-*` outputs. The CPU preset is installed at
`~/.config/harbor/cad/runtime-cpu.json` or `/etc/harbor/cad/runtime-cpu.json`.
CUDA, HIP and specialized simulation manifests are explicit selections.

```nix
programs.harbor.tools.cad.backends = ["cad-only" "thermal-worker"];
programs.harbor-cad.worker = {
  enable = true;
  profile = ./host-execution-profile.json;
};
```

The worker profile must identify the approved runtime, resource and path policy.
The Home Manager MCP socket defaults to the existing worker state path
`~/.local/state/harbor-cad/worker.sock`; override `tools.cad.mcp.socket` for an
existing differently located worker. Installing MCP does not start a worker.
Existing `programs.harborLlm.*`, `programs.harborRs.*` and `services.harbor-db.*`
configuration remains available.

## Integrated Home Manager

Import both root modules and set
`home-manager.users.<user>.programs.harbor.inheritSystemSelection = true` to
inherit host enable/framework values as defaults. A user can override the list
or individual tools. System-wide packages remain available to all users.

## Qualification

`checks.<system>.harbor-modules` evaluates real standalone Home Manager outputs,
overrides, platform selection and Linux NixOS/Home Manager integration. Linux
companion gates initialize the packaged Blender bridge, edit native geometry and
material graphs, inspect a viewport screenshot, and save/reopen a disposable file,
and run the preserved OpenPencil package and isolated desktop/MCP tests.
All changes to shared owners participate in the root Simit qualification graph.
