{
  pkgs,
  lib,
  tests,
}:
if pkgs.stdenv.hostPlatform.isLinux
then tests
else
  # APK derivations support Linux build hosts. Keep their successful-build
  # checks there, while retaining all platform-independent API checks here.
  builtins.removeAttrs tests [
    "mkAndroidFlavorTable-shape"
    "mkAndroidApk-shape"
    "mkAndroidApk-hermetic"
    "mkAndroidApk-hermetic-runtime"
  ]
  // {
    mkAndroidApk-unsupported-host = let
      unsupported =
        builtins.tryEval
        (lib.mkAndroidApk {
          inherit pkgs;
          androidSdk = pkgs.emptyDirectory;
          rustToolchain = pkgs.emptyDirectory;
          workspaceSrc = ./.;
          cargoPkg = "platform-policy-fixture";
          gradleModule = ":app";
          jniLibsDir = "android/app/src/main/jniLibs";
          apkOutPath = "android/app/build/outputs/apk/debug/app-debug.apk";
        }).drvPath;
    in
      assert !unsupported.success;
        pkgs.runCommand "check-mkAndroidApk-unsupported-host" {} "touch $out";
  }
