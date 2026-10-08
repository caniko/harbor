{
  pkgs,
  lib,
  ...
}: {
  settings.formatter.latexindent = {
    command = lib.mkDefault "${pkgs.texlive.withPackages (ps: [ps.latexindent])}/bin/latexindent";
    options = ["-w" "-s"];
    includes = ["*.tex" "*.sty" "*.cls" "*.bib"];
  };
}
