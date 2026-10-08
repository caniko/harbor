{
  description = "Reusable JavaScript and Bun infrastructure for Nix flakes";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    bun-overlay = {
      url = "github:alleneubank/bun-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    meta-harbor = {
      url = "git+https://github.com/caniko/meta-harbor.git?ref=trunk";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs @ {
    flake-parts,
    self,
    nixpkgs,
    bun-overlay,
    meta-harbor,
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
          inherit nixpkgs bun-overlay meta-harbor;
        };
      in {
        inherit lib;

        templates.default = {
          path = ./templates/default;
          description = "Bun project with js-harbor";
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
          meta = meta-harbor.lib;
        };

        formatter = pkgs.alejandra;
      };
    };
}
