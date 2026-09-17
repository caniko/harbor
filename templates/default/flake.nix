{
  description = "Foundry project powered by harbor-eth";

  inputs = {
    harbor-eth.url = "github:caniko/harbor-eth";
    nixpkgs.follows = "harbor-eth/nixpkgs";
    treefmt-nix.follows = "harbor-eth/treefmt-nix";
  };

  outputs = {
    harbor-eth,
    nixpkgs,
    treefmt-nix,
    ...
  }: let
    systems = ["x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin"];
  in {
    devShells = nixpkgs.lib.genAttrs systems (system: {
      default = harbor-eth.lib.mkEthDevShell {
        pkgs = import nixpkgs {inherit system;};
      };
    });

    formatter = nixpkgs.lib.genAttrs systems (system:
      (treefmt-nix.lib.evalModule nixpkgs.legacyPackages.${system} {
        imports = [
          harbor-eth.inputs.harbor-meta.treefmtModules.nix
          harbor-eth.inputs.harbor-meta.treefmtModules.toml
          harbor-eth.treefmtModules.solidity
        ];
        projectRootFile = "flake.nix";
      }).config.build.wrapper);
  };
}
