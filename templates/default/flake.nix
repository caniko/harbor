{
  description = "Foundry project powered by harbor-eth";

  inputs = {
    harbor-eth.url = "github:caniko/harbor-eth";
    nixpkgs.follows = "harbor-eth/nixpkgs";
    nixpkgs-darwin.follows = "harbor-eth/nixpkgs-darwin";
    treefmt-nix.follows = "harbor-eth/treefmt-nix";
  };

  outputs = {
    harbor-eth,
    nixpkgs,
    nixpkgs-darwin,
    treefmt-nix,
    ...
  }: let
    systems = ["x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin"];
    pkgsFor = system:
      import (
        if system == "x86_64-darwin"
        then nixpkgs-darwin
        else nixpkgs
      ) {inherit system;};
  in {
    devShells = nixpkgs.lib.genAttrs systems (system: {
      default = harbor-eth.lib.mkEthDevShell {
        pkgs = pkgsFor system;
      };
    });

    formatter = nixpkgs.lib.genAttrs systems (system:
      (treefmt-nix.lib.evalModule (pkgsFor system) {
        imports = [
          harbor-eth.inputs.harbor-meta.treefmtModules.nix
          harbor-eth.inputs.harbor-meta.treefmtModules.toml
          harbor-eth.treefmtModules.solidity
        ];
        projectRootFile = "flake.nix";
      }).config.build.wrapper);
  };
}
