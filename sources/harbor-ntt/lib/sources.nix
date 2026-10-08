{
  pkgs,
  pins,
}: let
  fetchGitHub = owner: repo: pin:
    pkgs.fetchFromGitHub {
      inherit owner repo;
      rev = pin.commit;
      hash = pin.source_hash;
    };
  inherit (pins) dependencies;
in {
  cli = fetchGitHub "wormhole-foundation" "native-token-transfers" dependencies.wormhole_ntt_cli;
  evm = fetchGitHub "wormhole-foundation" "native-token-transfers" dependencies.wormhole_ntt_evm;
  solana = fetchGitHub "wormhole-foundation" "native-token-transfers" dependencies.wormhole_ntt_solana;

  forgeStd = fetchGitHub "foundry-rs" "forge-std" dependencies.ntt_evm_transitive.forge_std;
  dsTest = fetchGitHub "dapphub" "ds-test" dependencies.ntt_evm_transitive.ds_test;
  nttOpenZeppelin = fetchGitHub "OpenZeppelin" "openzeppelin-contracts" dependencies.ntt_evm_transitive.openzeppelin_contracts;
  solidityBytesUtils = fetchGitHub "GNSPS" "solidity-bytes-utils" dependencies.ntt_evm_transitive.solidity_bytes_utils;
  wormholeSoliditySdk = fetchGitHub "wormhole-foundation" "wormhole-solidity-sdk" dependencies.ntt_evm_transitive.wormhole_solidity_sdk;
  wormhole = fetchGitHub "wormhole-foundation" "wormhole" dependencies.ntt_solana_git.wormhole;
  anchor = pkgs.fetchFromGitHub {
    owner = "coral-xyz";
    repo = "anchor";
    rev = pins.toolchains.ntt_solana.anchor_commit;
    hash = pins.toolchains.ntt_solana.anchor_source_hash;
  };
}
