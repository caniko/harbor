{
  pkgs,
  lib,
  system,
  packages,
}: let
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
    pkgs.runCommand "tex-harbor-profile-smoke-${system}" {
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
    name = "tex-harbor-pdflatex-smoke";
    src = ../fixtures/pdflatex;
    mainFile = "main.tex";
    engine = "pdflatex";
    profile = "cv";
  };

  lualatexSmoke = lib.mkLatexDocument {
    inherit pkgs;
    name = "tex-harbor-lualatex-smoke";
    src = ../fixtures/lualatex;
    mainFile = "main.tex";
    engine = "lualatex";
    profile = "article";
    shellEscape = true;
    nativeBuildInputs = [pkgs.inkscape];
  };

  xelatexSmoke = lib.mkLatexDocument {
    inherit pkgs;
    name = "tex-harbor-xelatex-smoke";
    src = ../fixtures/lualatex;
    mainFile = "main.tex";
    engine = "xelatex";
    profile = "article";
    shellEscape = true;
    nativeBuildInputs = [pkgs.inkscape];
  };

  nestedWorkingDirectorySmoke = lib.mkLatexDocument {
    inherit pkgs;
    name = "tex-harbor-nested-working-directory-smoke";
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
    name = "tex-harbor-conference-smoke";
    src = ../fixtures/conference;
    mainFile = "main.tex";
    engine = "pdflatex";
    profile = "conference";
  };

  anxPluginPandocImport = let
    pythonWithPlugin = pkgs.python3.withPackages (_: [packages.anx-plugin-pandoc]);
  in
    pkgs.runCommand "tex-harbor-anx-plugin-pandoc-import-${system}" {} ''
      ${pythonWithPlugin}/bin/python -c "from anx_plugin_pandoc import main; print('import OK')"
      mkdir -p "$out"
      echo ok > "$out/result"
    '';
in {
  inherit profileSmoke pdflatexSmoke lualatexSmoke xelatexSmoke nestedWorkingDirectorySmoke conferenceSmoke;
  anxPluginZenodoBuild = packages.anx-plugin-zenodo;
  inherit anxPluginPandocImport;
}
