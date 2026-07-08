{
  lib,
  bun-overlay,
}: let
  supportedSystems = [
    "x86_64-linux"
    "aarch64-linux"
    "x86_64-darwin"
    "aarch64-darwin"
  ];

  requireSupportedSystem = system:
    if builtins.elem system supportedSystems
    then system
    else throw "js-harbor.bun: unsupported system `${system}`; supported systems: ${lib.concatStringsSep ", " supportedSystems}";

  readPackageManagerVersion = {packageJson}: let
    parsed =
      if builtins.isPath packageJson || builtins.isString packageJson
      then builtins.fromJSON (builtins.readFile packageJson)
      else packageJson;
    packageManager = parsed.packageManager or null;
    match =
      if packageManager == null
      then null
      else builtins.match "bun@([^ ]+)" packageManager;
  in
    if match == null
    then throw "js-harbor.bun.readPackageManagerVersion: packageManager must be `bun@<version>`"
    else builtins.elemAt match 0;
in rec {
  inherit readPackageManagerVersion;

  mkBunPackage = {
    pkgs,
    version,
  }: let
    system = requireSupportedSystem pkgs.stdenv.hostPlatform.system;
  in
    (import "${bun-overlay}/default.nix" {
      inherit pkgs system;
      bunVersion = version;
    }).bun;

  mkBunToolchain = {
    pkgs,
    packageJson ? null,
    version ? null,
    extraPackages ? [],
  }: let
    resolvedVersion =
      if version != null
      then version
      else if packageJson != null
      then readPackageManagerVersion {inherit packageJson;}
      else throw "js-harbor.bun.mkBunToolchain: provide either `version` or `packageJson`";
    bun = mkBunPackage {
      inherit pkgs;
      version = resolvedVersion;
    };
  in {
    inherit bun;
    version = resolvedVersion;
    packages = [bun] ++ extraPackages;
    env = {
      BUN_VERSION = resolvedVersion;
    };
  };

  mkBunWorkspaceDeps = {
    pkgs,
    bun,
    src,
    packageJson,
    lockfile,
    filters ? [],
    hash,
    pname ? "bun-workspace-deps",
    version ? readPackageManagerVersion {inherit packageJson;},
    installFlags ? [],
    postInstallNormalize ? "",
  }:
    pkgs.stdenvNoCC.mkDerivation {
      inherit pname version src;

      nativeBuildInputs = [bun];
      dontConfigure = true;

      buildPhase = ''
        runHook preBuild
        export BUN_INSTALL_CACHE_DIR=$(mktemp -d)
        bun install \
          --frozen-lockfile \
          --ignore-scripts \
          --no-progress \
          ${lib.concatMapStringsSep " \\\n          " lib.escapeShellArg filters} \
          ${lib.concatMapStringsSep " \\\n          " lib.escapeShellArg installFlags}
        ${postInstallNormalize}
        runHook postBuild
      '';

      installPhase = ''
        runHook preInstall
        mkdir -p $out
        find . -type d -name node_modules -exec cp -R --parents {} $out \;
        runHook postInstall
      '';

      dontFixup = true;
      outputHashAlgo = "sha256";
      outputHashMode = "recursive";
      outputHash = hash;

      passthru = {
        inherit lockfile packageJson;
      };
    };
}
