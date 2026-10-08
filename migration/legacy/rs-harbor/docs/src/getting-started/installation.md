# Installation

Use `rs-harbor` as a flake input and follow its pinned shared dependencies from your project.

## Requirements

- Nix with flakes enabled
- `rust-overlay` applied to the `pkgs` you pass into `mkToolchain`
- A macOS SDK only if you want Darwin cross-compilation through osxcross

## Add the flake input

```nix
{
  inputs = {
    rs-harbor.url = "git+ssh://git@codeberg.org/caniko/rs-harbor.git";

    nixpkgs.follows = "rs-harbor/nixpkgs";
    rust-overlay.follows = "rs-harbor/rust-overlay";
    crane.follows = "rs-harbor/crane";
    flake-utils.follows = "rs-harbor/flake-utils";
  };
}
```

If you want a ready-made starting point for a Bevy project, `rs-harbor` also exports a template:

```bash
nix flake init -t git+ssh://git@codeberg.org/caniko/rs-harbor.git#bevy
```

For a working configuration after the input is added, continue to [Quick Start](./quick-start.md).
