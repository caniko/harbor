{pkgs, ...}: {
  projectRootFile = "flake.nix";

  programs.alejandra.enable = true;

  programs.taplo.enable = true;

  settings.formatter.latexindent = {
    command = "${pkgs.texlive.withPackages (ps: [ps.latexindent])}/bin/latexindent";
    options = ["-w" "-s"];
    includes = [
      "*.tex"
      "*.sty"
      "*.cls"
      "*.bib"
    ];
  };
}
