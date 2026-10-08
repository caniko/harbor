{
  description = "Anchor project powered by harbor-sol";

  inputs = {
    harbor-sol.url = "github:caniko/harbor-sol";
    nixpkgs.follows = "harbor-sol/nixpkgs";
    treefmt-nix.follows = "harbor-sol/treefmt-nix";
  };

  outputs = {
    harbor-sol,
    nixpkgs,
    treefmt-nix,
    ...
  }: let
    systems = ["x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin"];
  in {
    devShells = nixpkgs.lib.genAttrs systems (system: {
      default = harbor-sol.lib.mkSolanaDevShell {
        pkgs = import nixpkgs {
          inherit system;
          overlays = [harbor-sol.lib.rustOverlay];
        };
      };
    });

    formatter = nixpkgs.lib.genAttrs systems (system:
      (treefmt-nix.lib.evalModule nixpkgs.legacyPackages.${system} {
        imports = [
          harbor-sol.inputs.harbor-meta.treefmtModules.nix
          harbor-sol.inputs.harbor-meta.treefmtModules.toml
          harbor-sol.inputs.harbor-rs.treefmtModules.rust
        ];
        projectRootFile = "flake.nix";
      }).config.build.wrapper);
  };
}
