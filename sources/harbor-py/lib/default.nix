{
  nixpkgs,
  pyproject-nix,
  uv2nix,
  pyproject-build-systems,
  harbor-meta ? null,
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
      if harbor-meta != null
      then harbor-meta.lib.devShell
      else null;
  };
in
  pythonLib
  // rec {
    opencode =
      if harbor-meta != null
      then harbor-meta.lib.opencode
      else throw "harbor-py: opencode helpers require the harbor-meta flake input";

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
