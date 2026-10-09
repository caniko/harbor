{harbor}: {...}: {
  imports = [
    harbor.treefmtModules.core-nix
    harbor.treefmtModules.core-toml
    harbor.treefmtModules.javascript-javascript
  ];
  projectRootFile = "flake.nix";
  settings.global.excludes = [".crow/**"];
}
