# Qualify the isolated publisher with its committed input revisions. The
# reusable library's input graph stays independent of site tooling.
{
  pkgs,
  system,
}: let
  lock = builtins.fromJSON (builtins.readFile ../site/flake.lock);
  rootInputs = lock.nodes.${lock.root}.inputs;
  nixpkgsLock = lock.nodes.${rootInputs.nixpkgs}.locked;
  plinthLock = lock.nodes.${rootInputs.plinth}.locked;
  site = (import ../site/flake.nix).outputs {
    nixpkgs = builtins.getFlake "github:${nixpkgsLock.owner}/${nixpkgsLock.repo}/${nixpkgsLock.rev}";
    plinth = builtins.getFlake "git+${plinthLock.url}?ref=${plinthLock.ref}&rev=${plinthLock.rev}";
  };
in
  pkgs.runCommand "harbor-site-contract" {
    site = site.packages.${system}.site;
  } ''
    test -f "$site/index.html"
    test -f "$site/.domains"
    grep -qx 'harbor.tartanoglu.com' "$site/.domains"
    touch "$out"
  ''
