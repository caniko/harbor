{harbor-py}: {...}: {
  imports = [
    harbor-py.inputs.harbor-meta.treefmtModules.nix
    harbor-py.inputs.harbor-meta.treefmtModules.toml
    harbor-py.treefmtModules.python
  ];
  projectRootFile = "flake.nix";
}
