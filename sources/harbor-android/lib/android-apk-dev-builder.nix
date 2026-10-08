# mkAndroidApkDevBuilder :: {
#   pkgs,
#   flavors,
#   defaultFlavor?, defaultAbi?, defaultMode?,
#   androidDir?, cargoFeatures?, cargoNoDefaultFeatures?,
#   cargoNdkPlatform?, buildCommand?,
# } -> path
#
# Produce an impure developer helper script for staging Android APKs from the
# current checkout. This intentionally is not a derivation builder: it expects
# to run inside a dev shell with cargo-ndk, Gradle, Android SDK/NDK, and network
# access available, then leaves the APK in the project's normal Gradle output
# directory for device-oriented smoke loops.
#
# Cargo features, no-default-features, and Android API level may be set per
# flavor. Top-level values are fallbacks. This matters when callers override
# FLAVOR on a script whose default flavor has different Cargo requirements.
{packageTests}: {
  pkgs,
  flavors,
  packageName ? "android-apk",
  version ? "0.1.0",
  defaultFlavor ? "app",
  defaultAbi ? "arm64-v8a",
  defaultMode ? "debug",
  androidDir ? "android",
  cargoFeatures ? [],
  cargoNoDefaultFeatures ? false,
  cargoNdkPlatform ? null,
  buildCommand ? null,
  aapt2 ? null,
  lib ? pkgs.lib,
}: let
  validRelativePath = value:
    builtins.isString value
    && value != ""
    && !(lib.hasPrefix "/" value)
    && !(builtins.elem ".." (lib.splitString "/" value));
  validFeatures = features:
    builtins.isList features
    && builtins.all (feature: builtins.isString feature && feature != "") features;
  validPlatform = platform:
    platform == null || (builtins.isInt platform && platform > 0);
  resolveModeValue = value: mode:
    if builtins.isFunction value
    then value mode
    else if builtins.isAttrs value
    then value.${mode} or (throw "harbor-android: mkAndroidApkDevBuilder apkOutPath attrset is missing `${mode}`")
    else value;
in
  assert lib.assertMsg (builtins.isAttrs flavors && flavors != {})
  "harbor-android: mkAndroidApkDevBuilder requires a non-empty `flavors` attrset";
  assert lib.assertMsg (builtins.hasAttr defaultFlavor flavors)
  "harbor-android: mkAndroidApkDevBuilder defaultFlavor `${defaultFlavor}` is not in flavors";
  assert lib.assertMsg (builtins.elem defaultMode ["debug" "release"])
  "harbor-android: mkAndroidApkDevBuilder `defaultMode` must be \"debug\" or \"release\"";
  assert lib.assertMsg (builtins.isString defaultAbi && defaultAbi != "")
  "harbor-android: mkAndroidApkDevBuilder `defaultAbi` must be a non-empty string";
  assert lib.assertMsg (validRelativePath androidDir)
  "harbor-android: mkAndroidApkDevBuilder `androidDir` must be a non-empty relative path without `..`";
  assert lib.assertMsg (validFeatures cargoFeatures)
  "harbor-android: mkAndroidApkDevBuilder `cargoFeatures` must be a list of non-empty strings";
  assert lib.assertMsg (validPlatform cargoNdkPlatform)
  "harbor-android: mkAndroidApkDevBuilder `cargoNdkPlatform` must be null or a positive Android API level";
  assert lib.assertMsg (buildCommand == null || (builtins.isString buildCommand && buildCommand != ""))
  "harbor-android: mkAndroidApkDevBuilder `buildCommand` must be null or a non-empty string";
  assert lib.assertMsg (aapt2 == null || builtins.isPath aapt2 || builtins.isString aapt2)
  "harbor-android: mkAndroidApkDevBuilder `aapt2` must be null, a path, or a string"; let
    shellCase = lib.concatStringsSep "\n" (
      lib.mapAttrsToList (
        name: cfg: let
          flavorFeatures = cfg.cargoFeatures or cargoFeatures;
          flavorNoDefaultFeatures = cfg.cargoNoDefaultFeatures or cargoNoDefaultFeatures;
          flavorPlatform = cfg.cargoNdkPlatform or cargoNdkPlatform;
          apkOutPathDebug = resolveModeValue (cfg.apkOutPath or null) "debug";
          apkOutPathRelease = resolveModeValue (cfg.apkOutPath or null) "release";
        in
          assert lib.assertMsg (cfg ? cargoPkg && builtins.isString cfg.cargoPkg && cfg.cargoPkg != "")
          "harbor-android: mkAndroidApkDevBuilder flavor `${name}` requires a non-empty cargoPkg";
          assert lib.assertMsg (cfg ? gradleModule && builtins.isString cfg.gradleModule && cfg.gradleModule != ":" && lib.hasPrefix ":" cfg.gradleModule)
          "harbor-android: mkAndroidApkDevBuilder flavor `${name}` requires a Gradle module such as :app";
          assert lib.assertMsg (cfg ? jniLibsDir && validRelativePath cfg.jniLibsDir)
          "harbor-android: mkAndroidApkDevBuilder flavor `${name}` requires a relative jniLibsDir without `..`";
          assert lib.assertMsg (cfg ? apkOutPath && validRelativePath apkOutPathDebug && validRelativePath apkOutPathRelease)
          "harbor-android: mkAndroidApkDevBuilder flavor `${name}` requires relative debug/release apkOutPath values without `..`";
          assert lib.assertMsg (validFeatures flavorFeatures)
          "harbor-android: mkAndroidApkDevBuilder flavor `${name}` cargoFeatures must be a list of non-empty strings";
          assert lib.assertMsg (validPlatform flavorPlatform)
          "harbor-android: mkAndroidApkDevBuilder flavor `${name}` cargoNdkPlatform must be null or a positive Android API level"; ''
            ${lib.escapeShellArg name})
              cargo_pkg=${lib.escapeShellArg cfg.cargoPkg}
              gradle_module=${lib.escapeShellArg cfg.gradleModule}
              jni_libs_dir=${lib.escapeShellArg cfg.jniLibsDir}
              apk_out_path_debug=${lib.escapeShellArg apkOutPathDebug}
              apk_out_path_release=${lib.escapeShellArg apkOutPathRelease}
              cargo_features=${lib.escapeShellArg (lib.concatStringsSep "," flavorFeatures)}
              cargo_no_default=${
              if flavorNoDefaultFeatures
              then "1"
              else "0"
            }
              cargo_ndk_platform=${lib.escapeShellArg (
              if flavorPlatform == null
              then ""
              else toString flavorPlatform
            )}
              ;;
          ''
      )
      flavors
    );

    script = pkgs.writeShellScript "android-apk-dev-builder" ''
      set -euo pipefail

      if [ -z "''${ANDROID_NDK_HOME:-}" ]; then
        echo "error: ANDROID_NDK_HOME is unset. Run inside the Android dev shell." >&2
        exit 1
      fi
      if [ -z "''${ANDROID_SDK_ROOT:-}" ]; then
        echo "error: ANDROID_SDK_ROOT is unset. Run inside the Android dev shell." >&2
        exit 1
      fi
      export ANDROID_USER_HOME="''${ANDROID_USER_HOME:-$HOME/.android}"
      mkdir -p "$ANDROID_USER_HOME"
      ${lib.optionalString (aapt2 != null) ''
        if [ ! -x ${lib.escapeShellArg (toString aapt2)} ]; then
          echo "error: aapt2 is not executable: ${toString aapt2}" >&2
          exit 1
        fi
        export GRADLE_OPTS="''${GRADLE_OPTS:-} -Dorg.gradle.project.android.aapt2FromMavenOverride=${toString aapt2}"
      ''}
      for tool in cargo cargo-ndk gradle; do
        if ! command -v "$tool" >/dev/null; then
          echo "error: $tool not on PATH. Run inside the Android dev shell." >&2
          exit 1
        fi
      done

      flavor="''${FLAVOR:-${defaultFlavor}}"
      abi="''${ABI:-${defaultAbi}}"
      mode="''${MODE:-${defaultMode}}"

      case "$flavor" in
      ${shellCase}
        *)
          echo "error: unknown FLAVOR=$flavor (expected one of: ${lib.concatStringsSep ", " (builtins.attrNames flavors)})" >&2
          exit 1
          ;;
      esac

      cargo_args=(ndk -t "$abi" -o "$jni_libs_dir" build)
      mode_cap="Debug"
      apk_out_path="$apk_out_path_debug"
      if [ "$mode" = "release" ]; then
        cargo_args+=(--release)
        mode_cap="Release"
        apk_out_path="$apk_out_path_release"
      elif [ "$mode" != "debug" ]; then
        echo "error: unknown MODE=$mode (expected debug or release)" >&2
        exit 1
      fi
      if [ "$cargo_no_default" -eq 1 ]; then
        cargo_args+=(--no-default-features)
      fi
      if [ -n "$cargo_features" ]; then
        cargo_args+=(--features "$cargo_features")
      fi
      cargo_args+=(-p "$cargo_pkg")

      if [ -z "''${CARGO_NDK_PLATFORM:-}" ] && [ -n "$cargo_ndk_platform" ]; then
        export CARGO_NDK_PLATFORM="$cargo_ndk_platform"
      fi

      echo "[android-apk] flavor=$flavor abi=$abi mode=$mode CARGO_NDK_PLATFORM=''${CARGO_NDK_PLATFORM:-cargo-ndk-default}"
      echo "[android-apk] cargo ''${cargo_args[*]}"
      cargo "''${cargo_args[@]}"

      echo "[android-apk] gradle $gradle_module:assemble$mode_cap"
      (cd ${lib.escapeShellArg androidDir} && gradle "$gradle_module:assemble$mode_cap")

      if [ ! -f "$apk_out_path" ]; then
        echo "error: Gradle succeeded but APK was not found at $apk_out_path" >&2
        exit 1
      fi
      echo "[android-apk] APK at $apk_out_path"
    '';
  in
    script
    // {
      artifactBuilder = packageTests.mkArtifactBuilder {
        kind = "android-apk-dev-builder";
        inherit packageName version buildCommand;
        output = toString script;
        metadata = {
          inherit defaultFlavor defaultAbi defaultMode androidDir cargoFeatures cargoNoDefaultFeatures cargoNdkPlatform;
          flavors = builtins.attrNames flavors;
          helper = "mkAndroidApkDevBuilder";
        };
      };
    }
