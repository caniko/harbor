{
  description = "Reusable TeX Live profiles and LaTeX build helpers";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = {
    self,
    nixpkgs,
  }: let
    systems = [
      "x86_64-linux"
      "aarch64-linux"
      "aarch64-darwin"
    ];
    lib = import ./lib {inherit nixpkgs;};
    forAllSystems = f:
      nixpkgs.lib.genAttrs systems (system:
        f {
          inherit system;
          pkgs = nixpkgs.legacyPackages.${system};
        });
  in {
    inherit lib;

    packages = forAllSystems ({
      pkgs,
      system,
      ...
    }: {
      texlive-cv = lib.mkTexlive {
        inherit pkgs;
        profile = "cv";
      };
      texlive-article = lib.mkTexlive {
        inherit pkgs;
        profile = "article";
      };
      texlive-conference = lib.mkTexlive {
        inherit pkgs;
        profile = "conference";
      };
      texlive-editor = lib.mkTexlive {
        inherit pkgs;
        profile = "editor";
      };
      default = self.packages.${system}.texlive-article;
    });

    devShells = forAllSystems ({pkgs, ...}: {
      default = lib.mkTexDevShell {
        inherit pkgs;
        profile = "article";
        shellArgs = {
          packages = [pkgs.inkscape];
          shellHook = ''
            echo "tex-harbor article shell"
            echo "  latexmk -lualatex manuscript.tex"
          '';
        };
      };
      cv = lib.mkTexDevShell {
        inherit pkgs;
        profile = "cv";
      };
      conference = lib.mkTexDevShell {
        inherit pkgs;
        profile = "conference";
        shellArgs = {packages = [pkgs.inkscape];};
      };
      editor = lib.mkTexDevShell {
        inherit pkgs;
        profile = "editor";
      };
    });

    checks = forAllSystems ({
      pkgs,
      system,
      ...
    }:
      import ./checks {inherit pkgs lib system;});

    formatter = forAllSystems ({pkgs, ...}: pkgs.alejandra);
  };
}
