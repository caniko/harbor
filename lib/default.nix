{
  nixpkgs,
  harbor-meta ? null,
}: let
  nixLib = nixpkgs.lib;
  profiles = import ./profiles.nix;
  texlive = import ./texlive.nix {inherit profiles;};
  shell = import ./shell.nix {
    mkTexlive = texlive.mkTexlive;
    metaDevShell =
      if harbor-meta != null
      then harbor-meta.lib.devShell
      else null;
  };
  document = import ./document.nix {
    inherit nixLib;
    mkTexlive = texlive.mkTexlive;
  };
in {
  timezone = harbor-meta.lib.timezone;
  inherit profiles;
  inherit (texlive) mkTexlive;
  inherit (shell) mkTexDevShell;
  inherit (document) mkLatexDocument;
}
