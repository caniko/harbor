{
  self,
  pkgs,
  system,
  cross,
  mkCrossPackages ? self.lib.mkCrossPackages,
}: let
  toolchain = self.lib.mkToolchain {
    inherit pkgs;
    channel = "nightly";
    date = "2026-09-15";
    cache.enable = false;
  };
  packages = mkCrossPackages {
    inherit pkgs cross;
    inherit (toolchain) craneLib;
    pname = "musl-openssl";
    buildCache = null;
    commonArgs = {
      src = ./fixtures/musl-openssl;
      version = "0.1.0";
      strictDeps = true;
      doCheck = false;
      cargoExtraArgs = "--locked";
      nativeBuildInputs = [pkgs.perl];
    };
    targets = ["x86_64-linux-musl" "aarch64-linux-musl"];
  };
  validation = import ../lib/release-common.nix {inherit (pkgs) lib;};
  validate = target: machine:
    validation.staticElfValidation {
      readelf = "${pkgs.binutils}/bin/readelf";
      grep = "${pkgs.gnugrep}/bin/grep";
      path = "${packages.${target}}/bin/harbor-musl-openssl-fixture";
      inherit machine;
    };
in
  assert system == "x86_64-linux";
    pkgs.runCommand "check-mkCrossPackages-musl-openssl" {passthru = {inherit packages;};} ''
      set -euo pipefail
      ${validate "musl-openssl-x86_64-linux-musl" "Advanced Micro Devices X86-64"}
      ${validate "musl-openssl-aarch64-linux-musl" "AArch64"}
      mkdir -p "$out"
      ${packages.musl-openssl-x86_64-linux-musl}/bin/harbor-musl-openssl-fixture > "$out/execution.log"
      ${pkgs.gnugrep}/bin/grep -Fx HARBOR_MUSL_OPENSSL_OK "$out/execution.log"
    ''
