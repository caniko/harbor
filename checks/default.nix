{
  pkgs,
  lib,
  system,
  packages,
  self,
  nixpkgs,
  nixpkgs-darwin,
  treefmt-nix,
  git-hooks,
  meta,
}: let
  templateRoot = ../templates/default;
  templateFlake = builtins.readFile (templateRoot + "/flake.nix");
  templateSimit = builtins.fromTOML (builtins.readFile (templateRoot + "/simit.toml"));
  templateTreefmt = (treefmt-nix.lib.evalModule pkgs (import (templateRoot + "/nix/treefmt.nix") {harbor-tex = self;})).config;
  languageTreefmt = (treefmt-nix.lib.evalModule pkgs {imports = [self.treefmtModules.latex];}).config;
  templateHooks = builtins.readFile (templateRoot + "/nix/pre-commit.nix");
  profilePackages = {
    cv = lib.mkTexlive {
      inherit pkgs;
      profile = "cv";
    };
    article = lib.mkTexlive {
      inherit pkgs;
      profile = "article";
    };
    conference = lib.mkTexlive {
      inherit pkgs;
      profile = "conference";
    };
    editor = lib.mkTexlive {
      inherit pkgs;
      profile = "editor";
    };
  };

  profileSmoke =
    pkgs.runCommand "harbor-tex-profile-smoke-${system}" {
      nativeBuildInputs = [profilePackages.cv profilePackages.article profilePackages.editor];
    } ''
      command -v pdflatex
      command -v lualatex
      command -v latexmk
      command -v chktex
      command -v latexindent
      mkdir -p "$out"
      echo ok > "$out/result"
    '';

  pdflatexSmoke = lib.mkLatexDocument {
    inherit pkgs;
    name = "harbor-tex-pdflatex-smoke";
    src = ../templates/default;
    mainFile = "main.tex";
    engine = "pdflatex";
    profile = "cv";
  };

  lualatexSmoke = lib.mkLatexDocument {
    inherit pkgs;
    name = "harbor-tex-lualatex-smoke";
    src = ../fixtures/lualatex;
    mainFile = "main.tex";
    engine = "lualatex";
    profile = "article";
    shellEscape = true;
    nativeBuildInputs = [pkgs.inkscape];
  };

  xelatexSmoke = lib.mkLatexDocument {
    inherit pkgs;
    name = "harbor-tex-xelatex-smoke";
    src = ../fixtures/lualatex;
    mainFile = "main.tex";
    engine = "xelatex";
    profile = "article";
    shellEscape = true;
    nativeBuildInputs = [pkgs.inkscape];
  };

  nestedWorkingDirectorySmoke = lib.mkLatexDocument {
    inherit pkgs;
    name = "harbor-tex-nested-working-directory-smoke";
    src = ../fixtures;
    workingDirectory = "lualatex";
    mainFile = "main.tex";
    engine = "lualatex";
    profile = "article";
    shellEscape = true;
    nativeBuildInputs = [pkgs.inkscape];
  };

  conferenceSmoke = lib.mkLatexDocument {
    inherit pkgs;
    name = "harbor-tex-conference-smoke";
    src = ../fixtures/conference;
    mainFile = "main.tex";
    engine = "pdflatex";
    profile = "conference";
  };

  anxPluginPandocImport = let
    pythonWithPlugin = pkgs.python3.withPackages (_: [packages.anx-plugin-pandoc]);
  in
    pkgs.runCommand "harbor-tex-anx-plugin-pandoc-import-${system}" {} ''
      ${pythonWithPlugin}/bin/python -c "from anx_plugin_pandoc import main; print('import OK')"
      mkdir -p "$out"
      echo ok > "$out/result"
    '';
in
  assert templateSimit.flake
  == {
    scope = "full";
    mode = "custom";
    backend = "generic";
  };
  assert pkgs.lib.hasInfix "treefmt-nix.follows" templateFlake;
  assert pkgs.lib.hasInfix "git-hooks.follows" templateFlake;
  assert pkgs.lib.hasInfix "treefmtEval.config.build.check self" templateFlake;
  assert pkgs.lib.hasInfix "pre-commit-check.shellHook" templateFlake;
  assert templateTreefmt.settings.formatter ? latexindent;
  assert builtins.elem "*.tex" templateTreefmt.settings.formatter.latexindent.includes;
  assert builtins.attrNames languageTreefmt.settings.formatter == ["latexindent"];
  assert pkgs.lib.hasInfix "treefmt =" templateHooks;
  assert pkgs.lib.hasInfix "nix-flake-check" templateHooks; {
    inherit profileSmoke pdflatexSmoke lualatexSmoke xelatexSmoke nestedWorkingDirectorySmoke conferenceSmoke;
    anxPluginZenodoBuild = packages.anx-plugin-zenodo;
    inherit anxPluginPandocImport;

    template-default = meta.templateTests.mkCheck {
      inherit pkgs system;
      flakeNix = ../templates/default/flake.nix;
      inputs = {
        inherit nixpkgs nixpkgs-darwin treefmt-nix git-hooks;
        harbor-tex = self;
      };
      requiredFiles = [
        "flake.nix"
        "main.tex"
        "simit.toml"
        "nix/treefmt.nix"
        "nix/pre-commit.nix"
      ];
      requiredInputs = ["harbor-tex" "nixpkgs-darwin" "treefmt-nix" "git-hooks"];
      commands = ["pdflatex"];
      inherit (meta) devShellTests;
    };
  }
