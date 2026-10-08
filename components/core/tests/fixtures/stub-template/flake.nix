{
  description = "harbor-meta stub template";

  outputs = {
    self,
    nixpkgs,
  }: let
    systems = ["x86_64-linux" "aarch64-linux"];
  in {
    devShells = nixpkgs.lib.genAttrs systems (
      system: let
        pkgs = import nixpkgs {inherit system;};
      in {
        default = pkgs.mkShell {
          packages = [pkgs.hello];
          env.STUB_SHELL = "1";
          shellHook = ''
            echo stub
          '';
        };
      }
    );
  };
}
