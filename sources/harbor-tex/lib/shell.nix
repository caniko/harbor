{
  nixLib,
  mkTexlive,
  metaDevShell ? null,
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
    forwardedArgs = builtins.removeAttrs shellArgs ["packages" "env" "shellHook"];
  in
    if metaDevShell == null
    then throw "harbor-tex: mkTexDevShell requires the harbor-meta flake input"
    else
      metaDevShell.mkShell {
        inherit pkgs;
        packages = [texlive] ++ extraPackages ++ inheritedPackages;
        env = shellArgs.env or {};
        extraShellHook = shellArgs.shellHook or "";
        mkShellArgs = forwardedArgs;
      };
}
