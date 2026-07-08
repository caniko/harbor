{
  description = "Reusable JavaScript and Bun infrastructure for Nix flakes";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    bun-overlay = {
      url = "github:alleneubank/bun-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs @ {
    flake-parts,
    self,
    nixpkgs,
    bun-overlay,
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
          inherit nixpkgs bun-overlay;
        };
      in {
        inherit lib;
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
      in {
        packages = {
          inherit bun_1_3_14;
          default = bun_1_3_14;
        };

        checks = import ./checks {
          inherit pkgs system;
          lib = self.lib;
          inherit bun_1_3_14;
        };

        formatter = pkgs.alejandra;
      };
    };
}
