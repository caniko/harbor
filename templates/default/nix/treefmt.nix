{harbor-js}: {...}: {
  imports = [
    harbor-js.inputs.harbor-meta.treefmtModules.nix
    harbor-js.inputs.harbor-meta.treefmtModules.toml
    harbor-js.treefmtModules.javascript
  ];
  projectRootFile = "flake.nix";
  settings.global.excludes = [".crow/**"];
}
