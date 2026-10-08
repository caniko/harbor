# Quick Start

The fastest path is to create a toolchain, generate cross helpers, then feed both into `mkDevShells`.

```nix
{
  outputs = { self, nixpkgs, harbor-rs, flake-utils, rust-overlay, ... }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [(import rust-overlay)];
      };

      toolchain = harbor-rs.lib.mkToolchain { inherit pkgs; };
      cross = harbor-rs.lib.mkCross { inherit pkgs system; };
    in {
      devShells = harbor-rs.lib.mkDevShells {
        inherit pkgs cross;
        inherit (toolchain) craneLib;
      };
    });
}
```

This produces four shells:

- `nix develop` for native work
- `nix develop .#windows` for MinGW cross builds
- `nix develop .#macos` for osxcross, when a macOS SDK is configured
- `nix develop .#cross` for both Windows and macOS helpers together

If you need more control over the generated shell environment, read [mkDevShell and mkDevShells](../api/dev-shells.md). If you need macOS SDK setup, read [macOS SDK Initialization](../reference/macos-sdk.md).
