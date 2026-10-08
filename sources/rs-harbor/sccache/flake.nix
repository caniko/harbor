{
  description = "sccache user-daemon client — fail-closed wrapper and reusable env helpers";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs = {
    self,
    nixpkgs,
  }: let
    sccacheDefault = import ../lib/generated/sccache-default.nix;
    subLib = import ./lib {inherit nixpkgs sccacheDefault;};
    systems = ["x86_64-linux" "aarch64-linux"];
    forAllSystems = nixpkgs.lib.genAttrs systems;
  in {
    lib = subLib;

    packages = forAllSystems (system: let
      pkgs = import nixpkgs {inherit system;};
    in {
      sccache = pkgs.sccache;
      sccache-user-daemon-client = subLib.mkClientWrapper {
        inherit pkgs;
        sccachePackage = pkgs.sccache;
      };
    });

    # Re-export the same packages as a convenience devShell for testing
    devShells = forAllSystems (system: let
      pkgs = import nixpkgs {inherit system;};
    in {
      default = pkgs.mkShell {
        packages = with self.packages.${system}; [
          sccache
          sccache-user-daemon-client
        ];
      };
    });
  };
}
