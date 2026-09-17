{
  description = "Reusable JavaScript and Bun infrastructure for Nix flakes";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    bun-overlay = {
      url = "github:alleneubank/bun-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

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
    bun-overlay,
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

      flake = let
        lib = import ./lib {
          inherit nixpkgs bun-overlay harbor-meta;
        };
      in {
        inherit lib;
        treefmtModules.javascript = ./nix/treefmt/javascript.nix;

        templates.default = {
          path = ./templates/default;
          description = "Bun project with harbor-js";
        };
      };

      perSystem = {
        system,
        pkgs,
        ...
      }: let
        bun_1_3_14 = self.lib.bun.mkBunPackage {
          inherit pkgs;
          version = "1.3.14";
        };
        bun_1_3_14_baseline = self.lib.bun.mkBunPackage {
          inherit pkgs;
          version = "1.3.14";
          baseline = true;
        };
        treefmt = inputs.treefmt-nix.lib.evalModule pkgs {
          imports = [harbor-meta.treefmtModules.nix harbor-meta.treefmtModules.toml self.treefmtModules.javascript];
          projectRootFile = "flake.nix";
          settings.global.excludes = [".crow/**"];
        };
      in {
        packages =
          {
            inherit bun_1_3_14;
            default = bun_1_3_14;
          }
          // pkgs.lib.optionalAttrs (system == "x86_64-linux") {
            inherit bun_1_3_14_baseline;
          };

        checks = import ./checks {
          inherit pkgs system self;
          lib = self.lib;
          inherit bun_1_3_14 nixpkgs;
          inherit (inputs) treefmt-nix git-hooks;
          meta = harbor-meta.lib;
        };

        formatter = treefmt.config.build.wrapper;
      };
    };
}
