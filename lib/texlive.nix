{
  nixLib,
  profiles,
}: let
  resolveProfile = profile:
    if builtins.isString profile
    then profiles.${profile} or (throw "harbor-tex: unknown TeX profile `${profile}`")
    else profile;

  resolvePackage = ps: name:
    if builtins.hasAttr name ps
    then ps.${name}
    else throw "harbor-tex: TeX Live package `${name}` is unavailable in this nixpkgs revision";
in {
  mkTexlive = {
    pkgs,
    profile ? "article",
    extraPackages ? (_: []),
  }: let
    selected = resolveProfile profile;
    names = [selected.scheme] ++ selected.packages;
  in
    pkgs.texlive.withPackages (ps:
      (map (resolvePackage ps) names) ++ (extraPackages ps));
}
