{
  description = "Reusable TeX Live profiles and LaTeX build helpers";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    nixpkgs-darwin.url = "github:NixOS/nixpkgs/nixpkgs-26.05-darwin";
    harbor-meta = {
      url = "git+https://github.com/caniko/harbor-meta.git?ref=trunk";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    meta-harbor.follows = "harbor-meta";
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
    fleetix,
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
        src = ../..;
        cargoLock = {
          lockFile = ../../Cargo.lock;
          outputHashes."fleetix-0.4.0" = fleetix.narHash;
        };
        cargoBuildFlags = ["--package" "anx-plugin-zenodo"];
        cargoTestFlags = ["--package" "anx-plugin-zenodo"];
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
        inherit pkgs lib system self nixpkgs nixpkgs-darwin treefmt-nix git-hooks;
        packages = self.packages.${system};
        meta = harbor-meta.lib;
      });

    formatter = forAllSystems ({pkgs, ...}: pkgs.alejandra);
  };
}
