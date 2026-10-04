{harbor-meta}: let
  packageTests =
    if harbor-meta != null
    then harbor-meta.packageTests
    else throw "harbor-android: package-test helpers require the harbor-meta flake input";
in {
  timezone = harbor-meta.timezone;
  inherit packageTests;
  mkAndroidSdk = import ./android-sdk.nix;
  mkAndroidDevShell = import ./android-dev-shell.nix {inherit harbor-meta;};
  findLocalMavenCache = import ./android-maven-cache.nix;
  mkAndroidApk = import ./android-apk.nix {inherit packageTests;};
  mkAndroidApkDevBuilder = import ./android-apk-dev-builder.nix {inherit packageTests;};
  mkAndroidFlavorTable = import ./android-flavor-table.nix {inherit packageTests;};
  mkAndroidDeviceTools = import ./android-device.nix;
}
