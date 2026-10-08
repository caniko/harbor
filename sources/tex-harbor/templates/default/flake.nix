{
  description = "LaTeX project — powered by tex-harbor";

  inputs = {
    tex-harbor.url = "git+https://codeberg.org/caniko/tex-harbor.git?ref=trunk";
    nixpkgs.follows = "tex-harbor/nixpkgs";
  };

  outputs = {
    self,
    nixpkgs,
    tex-harbor,
  }: let
    systems = [
      "x86_64-linux"
      "aarch64-linux"
      "x86_64-darwin"
      "aarch64-darwin"
    ];
  in {
    packages = nixpkgs.lib.genAttrs systems (
      system: let
        pkgs = import nixpkgs {inherit system;};
      in {
        default = tex-harbor.lib.mkLatexDocument {
          inherit pkgs;
          name = "tex-harbor-template";
          src = ./.;
          mainFile = "main.tex";
          engine = "pdflatex";
          profile = "cv";
        };
      }
    );

    devShells = nixpkgs.lib.genAttrs systems (
      system: let
        pkgs = import nixpkgs {inherit system;};
      in {
        default = tex-harbor.lib.mkTexDevShell {
          inherit pkgs;
          profile = "cv";
        };
      }
    );
  };
}
