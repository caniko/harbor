{
  pkgs,
  system,
  lib,
  bun_1_3_14,
}: {
  bun-package-manager-version = let
    version = lib.bun.readPackageManagerVersion {
      packageJson = {
        packageManager = "bun@1.3.14";
      };
    };
  in
    assert version == "1.3.14";
      pkgs.runCommand "js-harbor-bun-package-manager-version" {} ''
        mkdir -p $out
        echo ok > $out/result
      '';

  bun-version = pkgs.runCommand "js-harbor-bun-version-${system}" {} ''
    test "$(${bun_1_3_14}/bin/bun --version)" = "1.3.14"
    mkdir -p $out
    echo ok > $out/result
  '';
}
