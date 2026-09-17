{
  description = "Reusable TeX Live profiles and LaTeX build helpers";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    nixpkgs-darwin.url = "github:NixOS/nixpkgs/nixpkgs-26.05-darwin";
    harbor-meta = {
      url = "git+https://github.com/caniko/harbor-meta.git?ref=trunk";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    git-hooks = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    nixpkgs-darwin,
    harbor-meta,
    treefmt-nix,
    git-hooks,
    ...
  }: let
    systems = [
      "x86_64-linux"
      "aarch64-linux"
      "x86_64-darwin"
      "aarch64-darwin"
    ];
    lib = import ./lib {inherit nixpkgs harbor-meta;};
    forAllSystems = f:
      nixpkgs.lib.genAttrs systems (system:
        f {
          inherit system;
          pkgs =
            import (
              if system == "x86_64-darwin"
              then nixpkgs-darwin
              else nixpkgs
            ) {
              inherit system;
            };
        });
  in {
    inherit lib;
    treefmtModules.latex = ./nix/treefmt/latex.nix;

    templates.default = {
      path = ./templates/default;
      description = "LaTeX project with harbor-tex";
    };

    packages = forAllSystems ({
      pkgs,
      system,
      ...
    }: let
      anx-plugin-zenodo = pkgs.rustPlatform.buildRustPackage {
        pname = "anx-plugin-zenodo";
        version = "0.1.0";
        src = ./plugins;
        cargoLock.lockFile = ./plugins/Cargo.lock;
        cargoBuildFlags = ["--package" "anx-plugin-zenodo"];
        nativeBuildInputs = [pkgs.pkg-config];
        buildInputs = [pkgs.openssl];
        meta = {
          description = "Zenodo archival plugin for the anx article toolchain";
          mainProgram = "anx-plugin-zenodo";
        };
      };

      anx-plugin-pandoc = pkgs.python3.pkgs.buildPythonPackage {
        pname = "anx-plugin-pandoc";
        version = "0.1.0";
        pyproject = true;
        src = ./plugins/pandoc;
        nativeBuildInputs = [pkgs.python3.pkgs.hatchling];
        pythonImportsCheck = ["anx_plugin_pandoc"];
        meta = {
          description = "Pandoc ODT export plugin for the anx article toolchain";
          mainProgram = "anx-plugin-pandoc";
        };
      };
    in {
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
      inherit anx-plugin-zenodo anx-plugin-pandoc;
      default = self.packages.${system}.texlive-article;
    });

    devShells = forAllSystems ({pkgs, ...}: {
      default = lib.mkTexDevShell {
        inherit pkgs;
        profile = "article";
        shellArgs = {
          packages = [pkgs.inkscape];
          shellHook = ''
            echo "harbor-tex article shell"
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
      import ./checks {
        inherit pkgs lib system self nixpkgs treefmt-nix git-hooks;
        packages = self.packages.${system};
        meta = harbor-meta.lib;
      });

    formatter = forAllSystems ({pkgs, ...}:
      (treefmt-nix.lib.evalModule pkgs {
        imports = [harbor-meta.treefmtModules.nix harbor-meta.treefmtModules.toml self.treefmtModules.latex];
        projectRootFile = "flake.nix";
      }).config.build.wrapper);
  };
}
