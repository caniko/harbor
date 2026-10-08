# Installation

Use `harbor-rs` as a flake input and follow its pinned shared dependencies from your project.

## Requirements

- Nix with flakes enabled
- `rust-overlay` applied to the `pkgs` you pass into `mkToolchain`
- nix-direnv, if you use the recommended `.envrc`
- A macOS SDK only if you want Darwin cross-compilation through osxcross

## Add the flake input

Pin a published `trunk` revision. Follow only inputs Harbor actually exposes:

```nix
{
  inputs = {
    harbor-rs.url = "git+https://github.com/caniko/harbor-rs.git?ref=trunk&rev=a5b436d24e175042c76d90fc4dbd31e92f7e1721";

    nixpkgs.follows = "harbor-rs/nixpkgs";
    rust-overlay.follows = "harbor-rs/rust-overlay";
    crane.follows = "harbor-rs/crane";
  };
}
```

Ready-made starting points:

```bash
nix flake init -t git+https://github.com/caniko/harbor-rs.git
nix flake init -t git+https://github.com/caniko/harbor-rs.git#bevy
```

For a working configuration after the input is added, continue to [Quick Start](./quick-start.md).
