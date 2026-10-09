{harbor}: {...}: {
  imports = [
    harbor.treefmtModules.core-nix
    harbor.treefmtModules.core-toml
    harbor.treefmtModules.tex-latex
  ];
  projectRootFile = "flake.nix";
}
