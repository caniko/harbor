{
  pkgs,
  lib,
  tests,
}: let
  runtimeTests =
    tests
    // {
      # The dev-builder uses the interactive user's Android home. Its sandbox
      # fixture needs a writable home instead of Nix's /homeless-shelter.
      mkAndroidApkDevBuilder-runtime = tests.mkAndroidApkDevBuilder-runtime.overrideAttrs (old: {
        buildCommand = ''
          export HOME="$PWD/home"
          ${old.buildCommand}
        '';
      });
    };
in
  if pkgs.stdenv.hostPlatform.isLinux
  then runtimeTests
  else
    # APK derivations support Linux build hosts. Keep their successful-build
    # checks there, while retaining all platform-independent API checks here.
    builtins.removeAttrs runtimeTests [
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
