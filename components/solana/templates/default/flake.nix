{
  description = "Anchor project powered by harbor-sol";

  inputs = {
    harbor.url = "git+https://github.com/caniko/harbor.git?ref=feat/harbor-monorepo-components&rev=3be813378c4b97f97edf06beb3bfed0a8d8acff1";
    nixpkgs.follows = "harbor/nixpkgs";
    nixpkgs-darwin.follows = "harbor/nixpkgs-darwin";
    treefmt-nix.follows = "harbor/treefmt-nix";
  };

  outputs = {
    harbor,
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
      ) {
        inherit system;
        overlays = [harbor.lib.blockchain.solana.rustOverlay];
      };
  in {
    devShells = nixpkgs.lib.genAttrs systems (system: {
      default = harbor.lib.blockchain.solana.mkSolanaDevShell {
        pkgs = pkgsFor system;
      };
    });

    formatter = nixpkgs.lib.genAttrs systems (system:
      (treefmt-nix.lib.evalModule (pkgsFor system) {
        imports = [
          harbor.treefmtModules.core-nix
          harbor.treefmtModules.core-toml
          harbor.treefmtModules.rust-rust
        ];
        projectRootFile = "flake.nix";
      }).config.build.wrapper);
  };
}
