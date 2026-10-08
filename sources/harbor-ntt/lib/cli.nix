{
  pkgs,
  pins,
  sources,
}: let
  toolchain = pins.toolchains.ntt_cli;

  bun = pkgs.stdenvNoCC.mkDerivation {
    pname = "bun";
    version = toolchain.bun;
    src = pkgs.fetchurl {
      url = toolchain.bun_archive_url;
      hash = toolchain.bun_archive_hash;
    };
    sourceRoot = "bun-linux-x64";
    nativeBuildInputs = [pkgs.autoPatchelfHook pkgs.unzip];
    buildInputs = [pkgs.openssl];
    installPhase = ''
      install -Dm755 bun "$out/bin/bun"
      ln -s bun "$out/bin/bunx"
    '';
  };

  node = pkgs.stdenvNoCC.mkDerivation {
    pname = "nodejs";
    version = toolchain.node;
    src = pkgs.fetchurl {
      url = toolchain.node_archive_url;
      hash = toolchain.node_archive_hash;
    };
    sourceRoot = "node-v${toolchain.node}-linux-x64";
    nativeBuildInputs = [pkgs.autoPatchelfHook];
    buildInputs = [pkgs.stdenv.cc.cc.lib];
    installPhase = ''
      cp -R . "$out"
    '';
  };

  nodeModules = pkgs.stdenvNoCC.mkDerivation {
    pname = "ntt-cli-node-modules";
    version = pins.dependencies.wormhole_ntt_cli.package_version;
    src = sources.cli;
    nativeBuildInputs = [bun pkgs.writableTmpDirAsHomeHook];
    impureEnvVars = pkgs.lib.fetchers.proxyImpureEnvVars ++ ["GIT_PROXY_COMMAND" "SOCKS_SERVER"];
    dontConfigure = true;
    buildPhase = ''
      export BUN_INSTALL_CACHE_DIR="$(mktemp -d)"
      bun ci --frozen-lockfile --ignore-scripts --no-progress
    '';
    installPhase = ''
      mkdir -p "$out"
      find . -type d -name node_modules -exec cp -R --parents {} "$out" \;
    '';
    dontFixup = true;
    outputHash = toolchain.dependency_cache_hash;
    outputHashAlgo = "sha256";
    outputHashMode = "recursive";
  };
in {
  inherit bun node nodeModules;

  package = pkgs.stdenvNoCC.mkDerivation {
    pname = "ntt-cli";
    version = pins.dependencies.wormhole_ntt_cli.package_version;
    src = sources.cli;
    nativeBuildInputs = [bun node pkgs.makeWrapper pkgs.writableTmpDirAsHomeHook];
    configurePhase = ''
      cp -R ${nodeModules}/. .
      patchShebangs .
    '';
    buildPhase = ''
      export BUN_INSTALL_CACHE_DIR="$TMPDIR/bun-cache"
      bun run build
    '';
    installPhase = ''
      mkdir -p "$out/share/ntt" "$out/bin"
      cp -R . "$out/share/ntt/"
      makeWrapper ${bun}/bin/bun "$out/bin/ntt" \
        --add-flags "$out/share/ntt/cli/src/index.ts" \
        --set NTT_DISABLE_AUTO_UPDATE 1
    '';
  };
}
