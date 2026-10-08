{
  description = "Anchor project powered by harbor-sol";

  inputs = {
    harbor-sol.url = "github:caniko/harbor-sol";
    nixpkgs.follows = "harbor-sol/nixpkgs";
    nixpkgs-darwin.follows = "harbor-sol/nixpkgs-darwin";
    treefmt-nix.follows = "harbor-sol/treefmt-nix";
  };

  outputs = {
    harbor-sol,
    nixpkgs,
    nixpkgs-darwin,
    treefmt-nix,
    ...
  }: let
    systems = ["x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin"];
    pkgsFor = system: import (if system == "x86_64-darwin" then nixpkgs-darwin else nixpkgs) {
      inherit system;
      overlays = [harbor-sol.lib.rustOverlay];
    };
  in {
    devShells = nixpkgs.lib.genAttrs systems (system: {
      default = harbor-sol.lib.mkSolanaDevShell {
        pkgs = pkgsFor system;
      };
    });

    formatter = nixpkgs.lib.genAttrs systems (system:
      (treefmt-nix.lib.evalModule (pkgsFor system) {
        imports = [
          harbor-sol.inputs.harbor-meta.treefmtModules.nix
          harbor-sol.inputs.harbor-meta.treefmtModules.toml
          harbor-sol.inputs.harbor-rs.treefmtModules.rust
        ];
        projectRootFile = "flake.nix";
      }).config.build.wrapper);
  };
}
