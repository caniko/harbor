{
  description = "Reusable Ethereum contract development infrastructure for Nix flakes";

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
  };

  outputs = {
    self,
    nixpkgs,
    harbor-meta,
    treefmt-nix,
    ...
  }: let
    systems = ["x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin"];
    forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f (import nixpkgs {inherit system;}));
    lib = import ./lib {inherit nixpkgs harbor-meta;};
  in {
    inherit lib;
    treefmtModules.solidity = ./nix/treefmt/solidity.nix;

    templates.default = {
      path = ./templates/default;
      description = "Foundry project with harbor-eth";
    };

    packages = forAllSystems (pkgs: {
      inherit (pkgs) foundry solc;
      default = pkgs.foundry;
    });

    devShells = forAllSystems (pkgs: {
      default = lib.mkEthDevShell {inherit pkgs;};
    });

    checks = forAllSystems (pkgs:
      import ./checks {
        inherit pkgs self nixpkgs;
        meta = harbor-meta.lib;
      });

    formatter = forAllSystems (pkgs:
      (treefmt-nix.lib.evalModule pkgs {
        imports = [harbor-meta.treefmtModules.nix harbor-meta.treefmtModules.toml self.treefmtModules.solidity];
        projectRootFile = "flake.nix";
      }).config.build.wrapper);
  };
}
