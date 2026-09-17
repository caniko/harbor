{
  pkgs,
  lib,
  ...
}: {
  settings.formatter.latexindent = {
    command = lib.mkDefault "${pkgs.latexindent}/bin/latexindent";
    options = ["-w" "-s"];
    includes = ["*.tex" "*.sty" "*.cls" "*.bib"];
  };
}
