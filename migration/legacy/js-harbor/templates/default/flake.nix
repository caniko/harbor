{
  description = "Bun project — powered by js-harbor";

  inputs = {
    js-harbor.url = "git+https://codeberg.org/caniko/js-harbor.git?ref=trunk";
    nixpkgs.follows = "js-harbor/nixpkgs";
  };

  outputs = {
    self,
    nixpkgs,
    js-harbor,
  }: let
    systems = [
      "x86_64-linux"
      "aarch64-linux"
      "x86_64-darwin"
      "aarch64-darwin"
    ];
  in {
    devShells = nixpkgs.lib.genAttrs systems (
      system: let
        pkgs = import nixpkgs {inherit system;};
      in {
        default = js-harbor.lib.mkBunDevShell {
          inherit pkgs;
          packageJson = ./package.json;
        };
      }
    );
  };
}
