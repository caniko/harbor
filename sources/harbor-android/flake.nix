{
  description = "Reusable Android SDK, NDK, and APK helpers for Nix flakes";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";

    harbor-meta = {
      url = "git+https://github.com/caniko/harbor-meta.git?ref=trunk";
      inputs.nixpkgs.follows = "nixpkgs";
    };


    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    git-hooks = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs @ {
    flake-parts,
    self,
    nixpkgs,
    harbor-meta,
    ...
  }:
    flake-parts.lib.mkFlake {inherit inputs;} {
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      flake = {
        treefmtModules = {
          java = ./nix/treefmt/java.nix;
          kotlin = ./nix/treefmt/kotlin.nix;
        };
        lib = import ./lib {
          harbor-meta = harbor-meta.lib;
        };

        templates.default = {
          path = ./templates/default;
          description = "Android project with harbor-android";
        };
      };

      perSystem = {pkgs, ...}: let
        treefmt = inputs.treefmt-nix.lib.evalModule pkgs {
          imports = [harbor-meta.treefmtModules.nix harbor-meta.treefmtModules.toml self.treefmtModules.java self.treefmtModules.kotlin];
          projectRootFile = "flake.nix";
        };
      in {
        checks = import ./checks {
          inherit pkgs self;
          lib = self.lib;
        };

        formatter = treefmt.config.build.wrapper;
      };
    };
}
