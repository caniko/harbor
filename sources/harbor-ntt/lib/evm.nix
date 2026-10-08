{
  pkgs,
  pins,
  sources,
}: let
  toolchain = pins.toolchains.ntt_evm;

  foundry = pkgs.stdenvNoCC.mkDerivation {
    pname = "foundry";
    version = toolchain.foundry;
    src = pkgs.fetchurl {
      url = toolchain.foundry_archive_url;
      hash = toolchain.foundry_archive_hash;
    };
    sourceRoot = ".";
    nativeBuildInputs = [pkgs.autoPatchelfHook];
    buildInputs = [pkgs.stdenv.cc.cc.lib];
    installPhase = ''
      mkdir -p "$out/bin"
      install -m755 forge cast chisel anvil "$out/bin/"
    '';
  };

  solc = pkgs.stdenvNoCC.mkDerivation {
    pname = "solc";
    version = toolchain.solidity;
    src = pkgs.fetchurl {
      url = toolchain.solc_archive_url;
      hash = toolchain.solc_archive_hash;
    };
    dontUnpack = true;
    nativeBuildInputs = [pkgs.autoPatchelfHook];
    buildInputs = [pkgs.stdenv.cc.cc.lib];
    installPhase = ''
      install -Dm755 "$src" "$out/bin/solc"
    '';
  };

  package =
    pkgs.runCommand "ntt-evm-raw-artifacts" {
      nativeBuildInputs = [foundry solc];
    } ''
      cp -R ${sources.evm}/evm work
      chmod -R u+w work
      rm -rf work/lib work/out work/cache
      mkdir -p work/lib
      ln -s ${sources.forgeStd} work/lib/forge-std
      ln -s ${sources.dsTest} work/lib/ds-test
      ln -s ${sources.nttOpenZeppelin} work/lib/openzeppelin-contracts
      ln -s ${sources.solidityBytesUtils} work/lib/solidity-bytes-utils
      ln -s ${sources.wormholeSoliditySdk} work/lib/wormhole-solidity-sdk
      cd work
      export HOME="$TMPDIR/home"
      export FOUNDRY_SOLC=${solc}/bin/solc
      mkdir -p "$HOME"
      forge build --offline
      mkdir -p "$out/artifacts" "$out/storage-layout"
      cp out/NttManager.sol/NttManager.json "$out/artifacts/"
      cp out/WormholeTransceiver.sol/WormholeTransceiver.json "$out/artifacts/"
      cp out/TransceiverStructs.sol/TransceiverStructs.json "$out/artifacts/"
      forge inspect NttManager storage-layout --json > "$out/storage-layout/NttManager.json"
      forge inspect WormholeTransceiver storage-layout --json > "$out/storage-layout/WormholeTransceiver.json"
    '';
in {
  inherit foundry package solc;
}
