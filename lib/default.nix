{
  nixpkgs,
  bun-overlay,
  harbor-meta ? null,
}: let
  bun = import ./bun.nix {
    lib = nixpkgs.lib;
    inherit bun-overlay;
    metaDevShell =
      if harbor-meta != null
      then harbor-meta.lib.devShell
      else null;
  };
in
  {
    inherit bun;
  }
  // {
    inherit (bun) mkBunPackage mkBunToolchain mkBunDevShell mkBunWorkspaceDeps readPackageManagerVersion;
  }
