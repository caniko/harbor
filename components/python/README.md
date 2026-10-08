# harbor-py

Reusable Python `uv2nix` and `pyproject-nix` infrastructure for Nix flakes.

`harbor-py` exposes a small `lib.*` API, following the same consumption style as
`harbor-rs`: downstream flakes keep project policy local while sharing the
boring Python/Nix plumbing.

## API

- `mkPkgs`, `forAllSystems`, `forPackageSystems`
- `mkPythonEnv`
- `loadUvWorkspace`, `mkPythonApplicationPackage`
- `mkUvHelper`, `mkUvDevShell`, `mkUvDevShells`
- `mkUvPythonSet`, `mkUvVirtualEnv`, `mkUvPackage`, `mkUvCheckEnv`, `mkUvAppPackage`
- `mkFfmpegCompat`
- `mkFfmpegTorchCodecAbiCheck`
- `pythonOverrides.addSetuptools`
- `pythonOverrides.addTorchRuntime`
- `pythonOverrides.addTorchCodecFfmpegRuntime`

`mkUvVirtualEnv` and `mkUvCheckEnv` return immutable uv2nix environments. Use
their `${env}/bin/python` interpreter rather than manually prepending a
`site-packages` path. They expose the same locations as passthroughs:
`pythonEnvironment`, `pythonInterpreter`, and `pythonSitePackages`.

`mkUvDevShell` `extraPackages` are Nix shell tools; they do not add Python
dependencies to the uv environment. Put Python dependencies in `pyproject.toml`
and `uv.lock`, or replace them through the pyproject-nix overlay. Keep one
provider for a given Python package instead of mixing Nix and uv copies through
`PYTHONPATH`.

Nix checks run against read-only source trees. Tool caches and reports must be
placed under `$TMPDIR` or `$out`, not in the source tree.

```bash
nix flake init -t github:caniko/harbor-py
```

## Minimal Usage

```nix
{
  inputs.harbor-py.url = "github:caniko/harbor-py";

  outputs = { self, nixpkgs, harbor-py, ... }:
    let
      py = harbor-py.lib;
    in {
      devShells = py.forAllSystems (system:
        let
          pkgs = py.mkPkgs { inherit system; };
        in {
          default = py.mkUvDevShell {
            inherit pkgs;
            uvExtra = "dev";
          };
        });
    };
}
```
