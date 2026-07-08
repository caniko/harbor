{
  nixpkgs,
  bun-overlay,
}: let
  bun = import ./bun.nix {
    lib = nixpkgs.lib;
    inherit bun-overlay;
  };
in
  {
    inherit bun;
  }
  // {
    inherit (bun) mkBunPackage mkBunToolchain mkBunWorkspaceDeps readPackageManagerVersion;
  }
