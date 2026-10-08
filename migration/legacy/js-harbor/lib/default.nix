{
  nixpkgs,
  bun-overlay,
  meta-harbor ? null,
}: let
  bun = import ./bun.nix {
    lib = nixpkgs.lib;
    inherit bun-overlay;
    metaDevShell =
      if meta-harbor != null
      then meta-harbor.lib.devShell
      else null;
  };
in
  {
    inherit bun;
  }
  // {
    inherit (bun) mkBunPackage mkBunToolchain mkBunDevShell mkBunWorkspaceDeps readPackageManagerVersion;
  }
