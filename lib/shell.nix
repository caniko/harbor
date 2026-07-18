{
  nixLib,
  mkTexlive,
}: {
  mkTexDevShell = {
    pkgs,
    profile ? "article",
    extraTexPackages ? (_: []),
    extraPackages ? [],
    shellArgs ? {},
  }: let
    texlive = mkTexlive {
      inherit pkgs profile;
      extraPackages = extraTexPackages;
    };
    inheritedPackages = shellArgs.packages or [];
    forwardedArgs = builtins.removeAttrs shellArgs ["packages"];
  in
    pkgs.mkShell (forwardedArgs
      // {
        packages = [texlive] ++ extraPackages ++ inheritedPackages;
      });
}
