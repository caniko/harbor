{
  description = "Shared harbor helpers for editor and agent tooling";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
  };

  outputs =
    inputs@{
      flake-parts,
      self,
      nixpkgs,
      ...
    }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [ "x86_64-linux" "aarch64-linux" ];

      flake = let
        lib = import ./lib { inherit nixpkgs; };
      in {
        inherit lib;
      };

      perSystem =
        { system, pkgs, ... }:
        let
          harborOpencode = import ./nix/harbor-opencode.nix {
            inherit pkgs;
            lib = self.lib;
          };
        in
        {
          packages = {
            harbor-opencode = harborOpencode;
            default = harborOpencode;
          };

          apps = {
            harbor-opencode = {
              type = "app";
              program = "${harborOpencode}/bin/harbor-opencode";
            };
            default = self.apps.${system}.harbor-opencode;
          };

          checks = import ./checks {
            inherit pkgs;
            lib = self.lib;
            inherit harborOpencode;
          };

          formatter = pkgs.alejandra;
        };
    };
}
