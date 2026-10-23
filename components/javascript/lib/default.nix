{
  nixpkgs,
  bun-overlay,
  harbor-meta ? null,
  opencodeLspLib ? null,
}: let
  mkTypescriptLsp = import ./typescript-lsp.nix {inherit opencodeLspLib;};
  node = import ./node.nix {
    inherit (nixpkgs) lib;
    metaDevShell =
      if harbor-meta != null
      then harbor-meta.lib.devShell
      else null;
    inherit mkTypescriptLsp;
  };
  bun = import ./bun.nix {
    inherit (nixpkgs) lib;
    inherit bun-overlay;
    metaDevShell =
      if harbor-meta != null
      then harbor-meta.lib.devShell
      else null;
    inherit mkTypescriptLsp;
  };
in
  {
    inherit bun node;
    timezone = harbor-meta.lib.timezone;
  }
  // {
    inherit (bun) mkBunPackage mkBunToolchain mkBunDevShell mkBunWorkspaceDeps readPackageManagerVersion;
    inherit (node) mkNodeToolchain mkNodeDevShell mkPnpmPackage readPnpmVersion;
  }
