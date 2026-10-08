{
  description = "Reusable Solana and Anchor development infrastructure for Nix flakes";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    harbor-meta = {
      url = "github:caniko/harbor-meta";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    harbor-rs = {
      url = "github:caniko/harbor-rs";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.harbor-meta.follows = "harbor-meta";
    };
  };

  outputs = {
    self,
    nixpkgs,
    harbor-meta,
    harbor-rs,
    treefmt-nix,
    ...
  }: let
    systems = ["x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin"];
    forAllSystems = f:
      nixpkgs.lib.genAttrs systems (system:
        f (import nixpkgs {
          inherit system;
          overlays = [self.lib.rustOverlay];
        }));
    lib = import ./lib {inherit harbor-meta harbor-rs;};
  in {
    inherit lib;

    templates.default = {
      path = ./templates/default;
      description = "Anchor project with harbor-sol";
    };

    packages = forAllSystems (pkgs: {
      inherit (pkgs) anchor solana-cli;
      cargo-build-sbf = lib.mkCargoBuildSbf {inherit pkgs;};
      default = pkgs.anchor;
    });

    devShells = forAllSystems (pkgs: {
      default = lib.mkSolanaDevShell {inherit pkgs;};
    });

    checks = forAllSystems (pkgs:
      import ./checks {
        inherit pkgs self nixpkgs;
        meta = harbor-meta.lib;
      });

    formatter = forAllSystems (pkgs:
      (treefmt-nix.lib.evalModule pkgs {
        imports = [harbor-meta.treefmtModules.nix harbor-meta.treefmtModules.toml harbor-rs.treefmtModules.rust];
        projectRootFile = "flake.nix";
      }).config.build.wrapper);
  };
}
