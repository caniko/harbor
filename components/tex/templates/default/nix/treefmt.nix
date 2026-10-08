{harbor-tex}: {...}: {
  imports = [
    harbor-tex.inputs.harbor-meta.treefmtModules.nix
    harbor-tex.inputs.harbor-meta.treefmtModules.toml
    harbor-tex.treefmtModules.latex
  ];
  projectRootFile = "flake.nix";
}
