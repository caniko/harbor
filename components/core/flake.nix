{
  description = "Shared harbor helpers for editor and agent tooling";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    # Suite-only nightly rustfmt: simit-shaped modules inject the nightly-only
    # skip_children flag, so the treefmt-scope regression suite cannot run on
    # stable rustfmt. Pinned by date (overlays only ever add manifests).
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs @ {
    flake-parts,
    self,
    nixpkgs,
    ...
  }:
    flake-parts.lib.mkFlake {inherit inputs;} {
      systems = ["x86_64-linux" "aarch64-linux"];

      flake = let
        lib = import ./lib {inherit nixpkgs;};
      in {
        inherit lib;
        treefmtModules = {
          nix = ./nix/treefmt/nix.nix;
          toml = ./nix/treefmt/toml.nix;
        };
      };

      perSystem = {
        system,
        pkgs,
        ...
      }: let
        # The harbor-meta package is deliberately profile-less: with an empty
        # profile registry only `detect`/`none` resolve (both render the
        # policy-only config), so `--kind rust` errors as unsupported.
        # Language-aware builds bind a profile registry in their own flake
        # (harbor-rs ships the rust one).
        harborOpencode = self.lib.opencode.mkCli {inherit pkgs;};
        treefmt = inputs.treefmt-nix.lib.evalModule pkgs (import ./nix/treefmt.nix);
      in {
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

        checks =
          (import ./checks {
            inherit pkgs nixpkgs;
            inherit (self) lib;
            inherit harborOpencode;
          })
          // {
            timezone = import ./checks/timezone.nix {
              inherit pkgs;
              inherit (self) lib;
            };
            timezone-consumers = let
              results = import ./tests/timezone-consumers.nix;
            in
              assert builtins.all (value: value) (builtins.attrValues results);
                pkgs.writeText "harbor-timezone-consumers.json" (builtins.toJSON results);
            dev-shell-bash = import ./checks/dev-shell-bash.nix {
              inherit pkgs;
              inherit (self.lib) devShell;
            };
            site = import ./checks/site.nix {inherit pkgs system;};
            treefmt-modules = import ./checks/treefmt.nix {
              inherit pkgs;
              inherit (inputs) treefmt-nix;
              modules = self.treefmtModules;
            };
            treefmt-scope = import ./checks/treefmt-scope.nix {
              inherit pkgs system nixpkgs;
              inherit (self) lib;
              inherit (inputs) treefmt-nix rust-overlay;
            };
            artifact-acceptance = import ./checks/artifact-acceptance.nix {
              inherit pkgs;
              inherit (self.lib) packageTests;
            };
          };

        formatter = treefmt.config.build.wrapper;
      };
    };
}
