{
  description = "Node and pnpm development — powered by harbor-js";

  inputs = {
    harbor.url = "git+https://github.com/caniko/harbor.git?ref=feat/harbor-monorepo-components&rev=3be813378c4b97f97edf06beb3bfed0a8d8acff1";
    nixpkgs.follows = "harbor/nixpkgs";
    nixpkgs-darwin.follows = "harbor/nixpkgs-darwin";
  };

  outputs = {
    nixpkgs,
    nixpkgs-darwin,
    harbor,
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
      default = harbor.lib.javascript.node.mkNodeDevShell {inherit pkgs;};
    });
  };
}
