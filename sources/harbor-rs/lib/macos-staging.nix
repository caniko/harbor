# mkMacosUniversalStager :: { pkgs, rsHarborCli } -> { stager, packages, ... }
#
# Produces the `stage-macos-universal` helper that takes cargo's per-target
# Mach-O outputs (and their split-debuginfo=packed `.dSYM` bundles) and
# stages them as both per-arch dist dirs and a universal slice via `lipo`.
#
# Implementation lives in the harbor-rs Rust CLI (`harbor-rs stage macos`).
# `rsHarborCli` (the harbor-rs CLI derivation) is required.
{
  pkgs,
  rsHarborCli,
}: let
  cargoMacosPackedDebuginfoSnippet = ''
    [profile.release]
    split-debuginfo = "packed"
  '';

  stager = pkgs.writeShellApplication {
    name = "stage-macos-universal";
    runtimeInputs = [rsHarborCli pkgs.llvmPackages.llvm];
    text = ''
      exec harbor-rs stage macos "$@"
    '';
  };
in {
  inherit cargoMacosPackedDebuginfoSnippet stager;
  packages = {stageMacosUniversal = stager;};
}
