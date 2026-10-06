{
  description = "Reusable JavaScript and Bun infrastructure for Nix flakes";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    nixpkgs-darwin.url = "github:NixOS/nixpkgs/nixpkgs-26.05-darwin";
    flake-parts.url = "github:hercules-ci/flake-parts";
    bun-overlay = {
      url = "github:alleneubank/bun-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    harbor-meta = {
      url = "git+https://github.com/caniko/harbor-meta.git?ref=feat/shared-timezone-env&rev=44a7c7cbb0cac897bcda2f5ef010dc73362bab9e";
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
    nixpkgs-darwin,
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
        templates.node = {
          path = ./templates/node;
          description = "Node and pnpm project with harbor-js";
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
        # Nixpkgs 26.11 retired Intel macOS; preserve this advertised platform
        # on the maintained Darwin compatibility branch.
        _module.args.pkgs =
          if system == "x86_64-darwin"
          then
            import nixpkgs-darwin {
              inherit system;
              # The compatibility branch keeps pnpm_10 on an insecure legacy
              # release; its maintained major-10 variant matches the main pin.
              overlays = [(_: prev: {pnpm_10 = prev.pnpm_10_latest;})];
            }
          else nixpkgs.legacyPackages.${system};
        devShells.default = self.lib.node.mkNodeDevShell {inherit pkgs;};
        devShells.node = self.lib.node.mkNodeDevShell {inherit pkgs;};
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
          inherit (self) lib;
          inherit bun_1_3_14 nixpkgs nixpkgs-darwin;
          inherit (inputs) treefmt-nix git-hooks;
          meta = harbor-meta.lib;
        };

        formatter = treefmt.config.build.wrapper;
      };
    };
}
