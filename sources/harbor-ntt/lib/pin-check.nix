{
  pkgs,
  pins,
  sources,
}: let
  inherit (pins) dependencies;
  checks = [
    {
      source = sources.cli;
      inherit (dependencies.wormhole_ntt_cli) lockfile lockfile_sha256;
    }
    {
      source = sources.cli;
      path = dependencies.wormhole_ntt_cli.version_marker;
      sha256 = dependencies.wormhole_ntt_cli.version_marker_sha256;
    }
    {
      source = sources.cli;
      path = dependencies.wormhole_ntt_cli.license_file;
      sha256 = dependencies.wormhole_ntt_cli.license_file_sha256;
    }
    {
      source = sources.evm;
      inherit (dependencies.wormhole_ntt_evm) lockfile lockfile_sha256;
    }
    {
      source = sources.evm;
      path = dependencies.wormhole_ntt_evm.version_marker;
      sha256 = dependencies.wormhole_ntt_evm.version_marker_sha256;
    }
    {
      source = sources.evm;
      path = dependencies.wormhole_ntt_evm.license_file;
      sha256 = dependencies.wormhole_ntt_evm.license_file_sha256;
    }
    {
      source = sources.solana;
      inherit (dependencies.wormhole_ntt_solana) lockfile lockfile_sha256;
    }
    {
      source = sources.solana;
      path = dependencies.wormhole_ntt_solana.version_marker;
      sha256 = dependencies.wormhole_ntt_solana.version_marker_sha256;
    }
    {
      source = sources.solana;
      path = dependencies.wormhole_ntt_solana.license_file;
      sha256 = dependencies.wormhole_ntt_solana.license_file_sha256;
    }
    {
      source = sources.anchor;
      path = "Cargo.lock";
      sha256 = pins.toolchains.ntt_solana.anchor_lockfile_sha256;
    }
    {
      source = sources.anchor;
      path = "LICENSE";
      sha256 = pins.toolchains.ntt_solana.anchor_license_file_sha256;
    }
  ];
  normalize = check:
    if check ? path
    then check
    else
      check
      // {
        path = check.lockfile;
        sha256 = check.lockfile_sha256;
      };
in
  pkgs.runCommand "harbor-ntt-pins" {} (
    pkgs.lib.concatMapStringsSep "\n" (raw: let
      check = normalize raw;
    in ''
      test "$(sha256sum ${check.source}/${check.path} | cut -d' ' -f1)" = ${pkgs.lib.escapeShellArg check.sha256}
    '')
    checks
    + ''
      touch "$out"
    ''
  )
