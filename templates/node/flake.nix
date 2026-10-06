{
  description = "Node and pnpm development — powered by harbor-js";

  inputs = {
    harbor-js.url = "github:caniko/harbor-js";
    nixpkgs.follows = "harbor-js/nixpkgs";
    nixpkgs-darwin.follows = "harbor-js/nixpkgs-darwin";
  };

  outputs = {
    nixpkgs,
    nixpkgs-darwin,
    harbor-js,
    ...
  }: {
    devShells = nixpkgs.lib.genAttrs ["x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin"] (system: let
      pkgs =
        if system == "x86_64-darwin"
        then
          import nixpkgs-darwin {
            inherit system;
            overlays = [(_: prev: {pnpm_10 = prev.pnpm_10_latest;})];
          }
        else nixpkgs.legacyPackages.${system};
    in {
      # Add packageJson = ./package.json once the project's exact pnpm pin
      # matches the selected package; use mkPnpmPackage for a different pin.
      default = harbor-js.lib.node.mkNodeDevShell {inherit pkgs;};
    });
  };
}
