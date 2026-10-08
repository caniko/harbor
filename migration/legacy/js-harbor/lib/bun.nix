{
  lib,
  bun-overlay,
  metaDevShell ? null,
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

  sources = builtins.fromJSON (builtins.readFile "${bun-overlay}/sources.json");

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

  linuxInterpreterPath = {
    aarch64-linux = "/lib/ld-linux-aarch64.so.1";
    x86_64-linux = "/lib64/ld-linux-x86-64.so.2";
  };

  mkBunFhsRunner = {pkgs}: let
    system = requireSupportedSystem pkgs.stdenv.hostPlatform.system;
  in
    pkgs.writeShellScriptBin "bun-fhs-run" (
      if pkgs.stdenvNoCC.hostPlatform.isLinux
      then ''
        set -euo pipefail
        loaderPath=${linuxInterpreterPath.${system}}
        loaderName=''${loaderPath##*/}
        exec ${pkgs.proot}/bin/proot \
          -b ${pkgs.glibc}/lib/$loaderName:$loaderPath \
          -b ${pkgs.glibc}:${pkgs.glibc} \
          -b ${pkgs.stdenv.cc.cc.lib}:${pkgs.stdenv.cc.cc.lib} \
          "$@"
      ''
      else ''
        exec "$@"
      ''
    );
in rec {
  inherit mkBunFhsRunner readPackageManagerVersion;

  mkBunPackage = {
    pkgs,
    version,
    baseline ? false,
  }: let
    system = requireSupportedSystem pkgs.stdenv.hostPlatform.system;
    versionData =
      if builtins.hasAttr version sources
      then sources.${version}
      else throw "js-harbor.bun.mkBunPackage: bun-overlay does not provide Bun version `${version}`";
    platform =
      if system == "x86_64-linux" && baseline
      then "x86_64-linux-baseline"
      else system;
    platformData =
      versionData.platforms.${platform}
        or (throw "js-harbor.bun.mkBunPackage: Bun ${version} is not available for `${platform}`");
    fhsRunner = mkBunFhsRunner {inherit pkgs;};
  in
    pkgs.stdenvNoCC.mkDerivation {
      pname = "bun";
      version = versionData.version;

      src = pkgs.fetchurl {
        inherit (platformData) url;
        inherit (platformData) sha256;
      };

      sourceRoot = ".";
      nativeBuildInputs =
        [
          pkgs.unzip
        ]
        ++ lib.optionals pkgs.stdenvNoCC.hostPlatform.isLinux [
          pkgs.makeWrapper
        ]
        ++ lib.optionals (pkgs.stdenvNoCC.hostPlatform.isDarwin or false) [
          pkgs.installShellFiles
        ];

      dontConfigure = true;
      dontBuild = true;

      installPhase =
        ''
          runHook preInstall
        ''
        + lib.optionalString pkgs.stdenvNoCC.hostPlatform.isLinux ''
          install -Dm755 */bun $out/libexec/bun/bun
          makeWrapper ${fhsRunner}/bin/bun-fhs-run $out/bin/bun \
            --add-flags "$out/libexec/bun/bun"
        ''
        + lib.optionalString (!pkgs.stdenvNoCC.hostPlatform.isLinux) ''
          install -Dm755 */bun $out/bin/bun
        ''
        + ''
          ln -s bun $out/bin/bunx
          runHook postInstall
        '';

      meta = {
        description = "Bun is a fast JavaScript runtime, package manager, bundler and test runner";
        homepage = "https://bun.sh";
        license = lib.licenses.mit;
        mainProgram = "bun";
        platforms = supportedSystems;
        sourceProvenance = with lib.sourceTypes; [binaryNativeCode];
      };

      passthru = {
        inherit fhsRunner;
      };
    };

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
    baseline =
      pkgs.stdenv.hostPlatform.system
      == "x86_64-linux"
      && !(pkgs.stdenv.hostPlatform.avx2Support or false);
    bun = mkBunPackage {
      inherit pkgs;
      version = resolvedVersion;
      inherit baseline;
    };
  in {
    inherit bun;
    version = resolvedVersion;
    packages = [bun] ++ extraPackages;
    env = {
      BUN_VERSION = resolvedVersion;
    };
  };

  mkBunDevShell = {
    pkgs,
    packageJson ? null,
    version ? null,
    extraPackages ? [],
    extraEnv ? {},
    extraShellHook ? "",
  }:
    if metaDevShell == null
    then throw "js-harbor.bun.mkBunDevShell requires the meta-harbor flake input"
    else let
      toolchain = mkBunToolchain {
        inherit pkgs packageJson version extraPackages;
      };
    in
      metaDevShell.mkShell {
        inherit pkgs;
        packages = toolchain.packages;
        env = toolchain.env // extraEnv;
        inherit extraShellHook;
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
