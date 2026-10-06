{
  description = "Reusable Android SDK, NDK, and APK helpers for Nix flakes";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    nixpkgs-darwin.url = "github:NixOS/nixpkgs/nixpkgs-26.05-darwin";
    flake-parts.url = "github:hercules-ci/flake-parts";

    harbor-meta = {
      url = "git+https://github.com/caniko/harbor-meta.git?ref=feat/shared-timezone-env&rev=1f272a44dea9dc531b30efb384720ddb46f083e3";
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

      perSystem = {
        pkgs,
        system,
        ...
      }: let
        platformNixpkgs =
          if system == "x86_64-darwin"
          then inputs.nixpkgs-darwin
          else inputs.nixpkgs;
        treefmt = inputs.treefmt-nix.lib.evalModule pkgs {
          imports = [harbor-meta.treefmtModules.nix harbor-meta.treefmtModules.toml self.treefmtModules.java self.treefmtModules.kotlin];
          projectRootFile = "flake.nix";
        };
      in {
        _module.args.pkgs = platformNixpkgs.legacyPackages.${system};

        checks = import ./checks/platform-policy.nix {
          inherit pkgs;
          lib = self.lib;
          tests = import ./checks {
            inherit pkgs self;
            lib = self.lib;
          };
        };

        formatter = treefmt.config.build.wrapper;
      };
    };
}
