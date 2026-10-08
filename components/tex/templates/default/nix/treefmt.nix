{pkgs, ...}: {
  projectRootFile = "flake.nix";

  programs.alejandra.enable = true;

  programs.taplo.enable = true;

  settings.formatter.latexindent = {
    command = "${pkgs.latexindent}/bin/latexindent";
    options = ["-w" "-s"];
    includes = [
      "*.tex"
      "*.sty"
      "*.cls"
      "*.bib"
    ];
  };
}
