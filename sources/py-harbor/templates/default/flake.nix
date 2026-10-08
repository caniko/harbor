{
  description = "Python uv project — powered by py-harbor";

  inputs = {
    py-harbor.url = "git+https://codeberg.org/caniko/py-harbor.git?ref=trunk";
    nixpkgs.follows = "py-harbor/nixpkgs";
  };

  outputs = {
    self,
    nixpkgs,
    py-harbor,
  }: let
    py = py-harbor.lib;
  in {
    devShells = nixpkgs.lib.genAttrs py.packageSystems (
      system: let
        pkgs = py.mkPkgs {inherit system;};
      in {
        default = py.mkUvDevShell {
          inherit pkgs;
        };
      }
    );
  };
}
