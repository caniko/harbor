{
  pkgs,
  lib,
  system,
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
      nativeBuildInputs = builtins.attrValues profilePackages;
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

  conferenceSmoke = lib.mkLatexDocument {
    inherit pkgs;
    name = "tex-harbor-conference-smoke";
    src = ../fixtures/conference;
    mainFile = "main.tex";
    engine = "pdflatex";
    profile = "conference";
  };
in {
  inherit profileSmoke pdflatexSmoke lualatexSmoke conferenceSmoke;
}
