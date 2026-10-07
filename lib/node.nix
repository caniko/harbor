{
  lib,
  metaDevShell ? null,
}: rec {
  readPnpmVersion = {packageJson}: let
    parsed =
      if builtins.isPath packageJson || builtins.isString packageJson
      then builtins.fromJSON (builtins.readFile packageJson)
      else packageJson;
    manager = parsed.packageManager or null;
    match =
      if builtins.isString manager
      then builtins.match "pnpm@([0-9]+[.][0-9]+[.][0-9]+)([+]sha(224|256|384|512)[.][A-Za-z0-9+/=]+)?" manager
      else null;
  in
    if match == null
    then throw "harbor-js.node.readPnpmVersion: packageManager must pin `pnpm@<major.minor.patch>`"
    else builtins.elemAt match 0;

  # Reuse Nixpkgs' Node-based pnpm builder for older pinned project managers.
  # Version and fixed-output hash are consumer policy, not a mutable Corepack
  # download in a shell hook. Node is supplied once to both the shell and pnpm.
  mkPnpmPackage = {
    pkgs,
    version,
    hash,
    nodejs ? pkgs.nodejs,
  }:
    pkgs.pnpm_10.override {
      inherit version hash;
      nodejs-slim = nodejs;
    };

  mkNodeToolchain = {
    pkgs,
    nodejs ? pkgs.nodejs,
    pnpm ? null,
    packageJson ? null,
    extraPackages ? [],
  }: let
    manager =
      if pnpm != null
      then pnpm
      else pkgs.pnpm_10.override {nodejs-slim = nodejs;};
    requiredVersion =
      if packageJson == null
      then manager.version
      else readPnpmVersion {inherit packageJson;};
  in
    assert lib.assertMsg (manager.version == requiredVersion)
    "harbor-js.node.mkNodeToolchain: pnpm ${manager.version} differs from packageManager ${requiredVersion}; supply a matching `pnpm` package (mkPnpmPackage accepts a version and fixed-output hash)"; {
      inherit nodejs;
      pnpm = manager;
      packages = [nodejs manager] ++ extraPackages;
      env = {
        # Never let pnpm fetch and execute a different manager on entering an
        # unrelated workspace. The selected Nix package remains authoritative.
        npm_config_manage_package_manager_versions = "false";
      };
    };

  mkNodeDevShell = {
    pkgs,
    nodejs ? pkgs.nodejs,
    pnpm ? null,
    packageJson ? null,
    extraPackages ? [],
    extraEnv ? {},
    extraShellHook ? "",
  }:
    if metaDevShell == null
    then throw "harbor-js.node.mkNodeDevShell requires the harbor-meta flake input"
    else let
      toolchain = mkNodeToolchain {inherit pkgs nodejs pnpm packageJson extraPackages;};
    in
      metaDevShell.mkShell {
        inherit pkgs extraShellHook;
        inherit (toolchain) packages;
        env = toolchain.env // extraEnv;
      };
}
