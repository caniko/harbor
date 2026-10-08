{
  nixpkgs,
  meta-harbor ? null,
}: let
  nixLib = nixpkgs.lib;
  profiles = import ./profiles.nix;
  texlive = import ./texlive.nix {inherit nixLib profiles;};
  shell = import ./shell.nix {
    inherit nixLib;
    mkTexlive = texlive.mkTexlive;
    metaDevShell =
      if meta-harbor != null
      then meta-harbor.lib.devShell
      else null;
  };
  document = import ./document.nix {
    inherit nixLib;
    mkTexlive = texlive.mkTexlive;
  };
in {
  inherit profiles;
  inherit (texlive) mkTexlive;
  inherit (shell) mkTexDevShell;
  inherit (document) mkLatexDocument;
}
