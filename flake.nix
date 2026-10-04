{
  description = "Reproducible Wormhole NTT development infrastructure for Nix flakes";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    harbor-meta = {
      url = "git+https://github.com/caniko/harbor-meta.git?ref=feat/shared-timezone-env&rev=44a7c7cbb0cac897bcda2f5ef010dc73362bab9e";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    harbor-meta,
    treefmt-nix,
    ...
  }: let
    system = "x86_64-linux";
    pkgs = nixpkgs.legacyPackages.${system};
    lib = import ./lib {inherit harbor-meta;};
  in {
    inherit lib;

    checks.${system} = import ./checks {
      inherit pkgs self;
    };

    # This repository contains Nix infrastructure; downstream NTT projects
    # compose Rust, JavaScript and Solidity modules for their own sources.
    formatter.${system} =
      (treefmt-nix.lib.evalModule pkgs {
        imports = [harbor-meta.treefmtModules.nix harbor-meta.treefmtModules.toml];
        projectRootFile = "flake.nix";
      }).config.build.wrapper;
  };
}
