{
  nixpkgs,
  pyproject-nix,
  uv2nix,
  pyproject-build-systems,
  meta-harbor ? null,
  opencodeLspLib ? null,
}: let
  nixLib = nixpkgs.lib;
  pythonLib = import ./python.nix {
    inherit
      nixLib
      pyproject-nix
      uv2nix
      pyproject-build-systems
      opencodeLspLib
      ;
    metaDevShell =
      if meta-harbor != null
      then meta-harbor.lib.devShell
      else null;
  };
in
  pythonLib
  // rec {
    opencode =
      if meta-harbor != null
      then meta-harbor.lib.opencode
      else throw "py-harbor: opencode helpers require the meta-harbor flake input";

    allSystems = nixLib.systems.flakeExposed;
    packageSystems = [
      "x86_64-linux"
      "aarch64-darwin"
    ];

    forAllSystems = f: nixLib.genAttrs allSystems f;
    forPackageSystems = f: nixLib.genAttrs packageSystems f;

    mkPkgs = {
      system,
      overlays ? [],
      config ? {},
    }:
      import nixpkgs {
        inherit system overlays;
        config =
          {
            allowUnfree = true;
          }
          // config;
      };
  }
