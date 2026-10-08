{harbor-meta}: let
  packageTests =
    if harbor-meta != null
    then harbor-meta.packageTests
    else throw "harbor-android: package-test helpers require the harbor-meta flake input";
in {
  inherit packageTests;
  mkAndroidSdk = import ./android-sdk.nix;
  mkAndroidDevShell = import ./android-dev-shell.nix;
  findLocalMavenCache = import ./android-maven-cache.nix;
  mkAndroidApk = import ./android-apk.nix {inherit packageTests;};
  mkAndroidApkDevBuilder = import ./android-apk-dev-builder.nix {inherit packageTests;};
  mkAndroidFlavorTable = import ./android-flavor-table.nix {inherit packageTests;};
  mkAndroidDeviceTools = import ./android-device.nix;
}
