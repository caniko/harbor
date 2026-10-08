{
  pkgs,
  pins,
  sources,
  anchorRustToolchain,
}: let
  toolchain = pins.toolchains.ntt_solana;

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

  solana = pkgs.stdenvNoCC.mkDerivation {
    pname = "solana-cli";
    version = toolchain.solana;
    src = pkgs.fetchurl {
      url = toolchain.solana_archive_url;
      hash = toolchain.solana_archive_hash;
    };
    platformTools = pkgs.fetchurl {
      url = "https://github.com/anza-xyz/platform-tools/releases/download/v${toolchain.platform_tools}/platform-tools-linux-x86_64.tar.bz2";
      hash = toolchain.platform_tools_archive_hash;
    };
    criterion = pkgs.fetchurl {
      url = "https://github.com/Snaipe/Criterion/releases/download/v${toolchain.criterion}/criterion-v${toolchain.criterion}-linux-x86_64.tar.bz2";
      hash = toolchain.criterion_archive_hash;
    };
    sourceRoot = "solana-release";
    nativeBuildInputs = [pkgs.autoPatchelfHook pkgs.makeWrapper];
    buildInputs = [pkgs.libxml2 pkgs.ncurses pkgs.openssl pkgs.stdenv.cc.cc.lib pkgs.udev pkgs.xz pkgs.zlib];
    installPhase = ''
      cp -R . "$out"
      mkdir -p "$out/bin/sdk/sbf/dependencies/platform-tools" "$out/bin/sdk/sbf/dependencies/criterion"
      tar --strip-components 1 -xjf "$platformTools" -C "$out/bin/sdk/sbf/dependencies/platform-tools"
      tar --strip-components 1 -xjf "$criterion" -C "$out/bin/sdk/sbf/dependencies/criterion"
      touch "$out/bin/sdk/sbf/dependencies/platform-tools-v${toolchain.platform_tools}.md"
      touch "$out/bin/sdk/sbf/dependencies/criterion-v${toolchain.criterion}.md"
      rm -rf "$out/bin/perf-libs"
      rm -f "$out/bin/sdk/sbf/dependencies/platform-tools/llvm/lib/"liblldb*
      rm -f "$out/bin/sdk/sbf/dependencies/platform-tools/llvm/bin/lldb"*
      rm -rf "$out/bin/sdk/sbf/dependencies/platform-tools/llvm/lib/python3.8"
    '';
  };

  anchor =
    (pkgs.makeRustPlatform {
      cargo = anchorRustToolchain;
      rustc = anchorRustToolchain;
    }).buildRustPackage {
      pname = "anchor-cli";
      version = toolchain.anchor;
      src = sources.anchor;
      cargoHash = toolchain.anchor_cargo_hash;
      cargoBuildFlags = ["-p" "anchor-cli"];
      cargoTestFlags = ["-p" "anchor-cli"];
      doCheck = false;
      nativeBuildInputs = [pkgs.pkg-config];
      buildInputs = [pkgs.openssl];
    };

  cargoDeps = pkgs.rustPlatform.fetchCargoVendor {
    pname = "ntt-solana";
    version = pins.dependencies.wormhole_ntt_solana.tag;
    src = sources.solana;
    cargoRoot = "solana";
    hash = toolchain.solana_cargo_vendor_hash;
  };

  sbfPackage = {
    name,
    flags,
    includeFixtures ? false,
  }:
    pkgs.stdenv.mkDerivation {
      inherit cargoDeps;
      pname = name;
      version = pins.dependencies.wormhole_ntt_solana.tag;
      src = sources.solana;
      sourceRoot = "source/solana";
      nativeBuildInputs = [
        pkgs.pkg-config
        pkgs.rustPlatform.cargoSetupHook
        pkgs.rustup
        anchorRustToolchain
        solana
      ];
      buildInputs = [pkgs.openssl];
      buildPhase = ''
        runHook preBuild
        export HOME="$TMPDIR/home"
        export CARGO_NET_OFFLINE=true
        export RUSTUP_HOME="$TMPDIR/rustup"
        export PATH=${pkgs.rustup}/bin:$PATH
        mkdir -p "$HOME/.cache/solana/v${toolchain.platform_tools}" "$RUSTUP_HOME"
        ln -s ${solana}/bin/sdk/sbf/dependencies/platform-tools "$HOME/.cache/solana/v${toolchain.platform_tools}/platform-tools"
        cp -R ${solana}/bin/sdk/sbf "$TMPDIR/sbf-sdk"
        chmod -R u+w "$TMPDIR/sbf-sdk"
        rm -rf "$TMPDIR/sbf-sdk/dependencies/platform-tools"
        export SBF_SDK_PATH="$TMPDIR/sbf-sdk"
        rustup toolchain link solana ${solana}/bin/sdk/sbf/dependencies/platform-tools/rust
        export RUSTUP_TOOLCHAIN=solana
        (cd programs/example-native-token-transfers && cargo build-sbf --offline -- --locked ${flags})
        (cd programs/ntt-transceiver && cargo build-sbf --offline -- --locked ${flags})
        runHook postBuild
      '';
      installPhase = ''
        runHook preInstall
        mkdir -p "$out/programs"
        install -m444 target/deploy/example_native_token_transfers.so "$out/programs/"
        install -m444 target/deploy/ntt_transceiver.so "$out/programs/"
        ${pkgs.lib.optionalString includeFixtures ''
          mkdir -p "$out/wormhole-core/accounts"
          install -m444 programs/example-native-token-transfers/tests/fixtures/mainnet_core_bridge.so "$out/wormhole-core/"
          cp tests/accounts/mainnet/*.json "$out/wormhole-core/accounts/"
        ''}
        runHook postInstall
      '';
    };
in {
  inherit anchor cargoDeps node solana;
  mkSbfPackage = sbfPackage;

  mainnetFixture = sbfPackage {
    name = "ntt-solana-mainnet-raw";
    flags = "--features mainnet";
    includeFixtures = true;
  };
  devnetArtifacts = sbfPackage {
    name = "ntt-solana-devnet-raw";
    flags = "--no-default-features --features solana-devnet";
  };
}
