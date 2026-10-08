# mkAndroidApk :: {
#   pkgs, androidSdk, rustToolchain, workspaceSrc,
#   cargoPkg, gradleModule, jniLibsDir, apkOutPath,
#   abi?, mode?, pname?, version?, androidDir?, ndkVersion?,
#   cargoNdkPlatform?, cargoVendorDir?, buildCommand?,
#   cargoFeatures?, cargoNoDefaultFeatures?,
#   mavenCacheTar?, gradleDeps?,
#   cargoNdk?, jdk?, gradle?, perl?, lib?,
# } -> derivation
#
# Build an Android APK from a cargo workspace member that exposes a `cdylib`,
# packaging the resulting `.so` via a Gradle module that uses AndroidX
# `GameActivity` (typical for Bevy 0.18 + `android-game-activity`).
#
# Two build modes:
#
#   - **Hermetic**: pass both `cargoVendorDir` (the output of
#     `craneLib.vendorCargoDeps`) and `mavenCacheTar` (a path or store-path
#     tarball whose root is `files-2.1/` from a prior `gradle assemble` run).
#     Cargo and Gradle then run offline. Both inputs are required: a Maven-only
#     cache is not a hermetic Rust build.
#
#   - **Impure**: omit `mavenCacheTar`. Gradle resolves Maven artifacts from the
#     network; Cargo also uses the network unless `cargoVendorDir` is supplied.
#     The derivation requires `__noChroot = true;` (and therefore
#     `--option sandbox false` or a Nix configured to permit it).
#
# Reuses the workspace-pinned `rustToolchain` (see `mkToolchain`) so the
# `cdylib` is built with the exact same compiler as the rest of the workspace;
# the project's `Cargo.lock` therefore stays load-bearing. The toolchain must
# include the Rust target corresponding to `abi`.
#
# Example consumer:
#
#   cargoVendorDir = toolchain.craneLib.vendorCargoDeps { src = workspaceSrc; };
#   packages.android-apk-debug = rs-harbor.lib.mkAndroidApk {
#     inherit pkgs androidSdk rustToolchain workspaceSrc cargoVendorDir;
#     cargoPkg = "my-game";
#     gradleModule = ":app";
#     jniLibsDir = "android/app/src/main/jniLibs";
#     apkOutPath = "android/app/build/outputs/apk/debug/app-debug.apk";
#     cargoNdkPlatform = 28; # Keep equal to Gradle's minSdk.
#     buildCommand = "nix build .#android-apk-debug";
#     mavenCacheTar = ./gradle-cache.tar;
#   };
{packageTests}: {
  pkgs,
  androidSdk,
  rustToolchain,
  workspaceSrc,
  # Cargo workspace member whose cdylib produces `lib<cargoPkg>.so`.
  cargoPkg,
  # Gradle subproject path, e.g. ":app" or ":test-peer".
  gradleModule,
  # Where `cargo ndk -o` should drop the `.so` so Gradle picks it up.
  jniLibsDir,
  # Final relative path inside the workspace where Gradle emits the APK.
  apkOutPath,
  abi ? "arm64-v8a",
  mode ? "debug",
  pname ? "android-apk",
  version ? "0.1.0",
  cargoFeatures ? [],
  cargoNoDefaultFeatures ? false,
  androidDir ? "android",
  # Android API level passed through CARGO_NDK_PLATFORM. Keep this equal to
  # the Gradle module's minSdk. Null leaves cargo-ndk's default unchanged.
  cargoNdkPlatform ? null,
  # Output of craneLib.vendorCargoDeps. When supplied, Cargo runs offline.
  cargoVendorDir ? null,
  # Exact downstream flake command, if known. Null is preferable to guessing
  # an output attribute that the helper cannot see.
  buildCommand ? null,
  # Toolchain / tool overrides.
  ndkVersion ? "29.0.14206865",
  cargoNdk ? pkgs.cargo-ndk,
  jdk ? pkgs.jdk21,
  gradle ? pkgs.gradle,
  # openssl-sys builds vendored OpenSSL for Android and invokes Configure.
  perl ? pkgs.perl,
  lib ? pkgs.lib,
  # Maven cache tarball whose root is `files-2.1/`.
  mavenCacheTar ? null,
  # Reserved for gradle2nix once its manifest materialization is implemented.
  # Reject non-null values instead of silently claiming hermetic support.
  gradleDeps ? null,
}: let
  validRelativePath = value:
    builtins.isString value
    && value != ""
    && !(lib.hasPrefix "/" value)
    && !(builtins.elem ".." (lib.splitString "/" value));
in
  assert lib.assertMsg (builtins.isString cargoPkg && cargoPkg != "")
  "rs-harbor: mkAndroidApk requires a non-empty `cargoPkg` workspace cdylib package name";
  assert lib.assertMsg (builtins.isString gradleModule && gradleModule != ":" && lib.hasPrefix ":" gradleModule)
  "rs-harbor: mkAndroidApk `gradleModule` must look like \"app\" or \"test-peer\" with a leading colon";
  assert lib.assertMsg (builtins.elem mode ["debug" "release"])
  "rs-harbor: mkAndroidApk `mode` must be \"debug\" or \"release\", got ${mode}";
  assert lib.assertMsg (rustToolchain != null)
  "rs-harbor: mkAndroidApk requires `rustToolchain` — pass `(mkToolchain { ... }).rustToolchain`";
  assert lib.assertMsg (builtins.isList cargoFeatures && builtins.all (feature: builtins.isString feature && feature != "") cargoFeatures)
  "rs-harbor: mkAndroidApk `cargoFeatures` must be a list of non-empty strings";
  assert lib.assertMsg (validRelativePath androidDir)
  "rs-harbor: mkAndroidApk `androidDir` must be a non-empty relative path without `..`";
  assert lib.assertMsg (validRelativePath jniLibsDir)
  "rs-harbor: mkAndroidApk `jniLibsDir` must be a non-empty relative path without `..`";
  assert lib.assertMsg (validRelativePath apkOutPath)
  "rs-harbor: mkAndroidApk `apkOutPath` must be a non-empty relative path without `..`";
  assert lib.assertMsg (builtins.isString abi && abi != "")
  "rs-harbor: mkAndroidApk `abi` must be a non-empty string";
  assert lib.assertMsg (builtins.isString ndkVersion && ndkVersion != "")
  "rs-harbor: mkAndroidApk `ndkVersion` must be a non-empty string";
  assert lib.assertMsg (cargoNdkPlatform == null || (builtins.isInt cargoNdkPlatform && cargoNdkPlatform > 0))
  "rs-harbor: mkAndroidApk `cargoNdkPlatform` must be null or a positive Android API level";
  assert lib.assertMsg (buildCommand == null || (builtins.isString buildCommand && buildCommand != ""))
  "rs-harbor: mkAndroidApk `buildCommand` must be null or a non-empty string";
  assert lib.assertMsg (gradleDeps == null)
  "rs-harbor: mkAndroidApk `gradleDeps` is not implemented; use `mavenCacheTar` plus `cargoVendorDir`";
  assert lib.assertMsg (mavenCacheTar == null || cargoVendorDir != null)
  "rs-harbor: mkAndroidApk `mavenCacheTar` also requires `cargoVendorDir` from `craneLib.vendorCargoDeps`; Maven-only caching is not hermetic"; let
    modeCap =
      if mode == "debug"
      then "Debug"
      else "Release";

    ndkRoot = "${androidSdk}/libexec/android-sdk/ndk/${ndkVersion}";
    cargoHermetic = cargoVendorDir != null;
    gradleHermetic = mavenCacheTar != null;
    hermetic = cargoHermetic && gradleHermetic;

    cargoArgs =
      ["ndk" "-t" abi "-o" jniLibsDir "build"]
      ++ lib.optional (mode == "release") "--release"
      ++ lib.optional cargoNoDefaultFeatures "--no-default-features"
      ++ lib.optionals (cargoFeatures != []) ["--features" (lib.concatStringsSep "," cargoFeatures)]
      ++ ["-p" cargoPkg];
    gradleArgs =
      ["${gradleModule}:assemble${modeCap}" "--no-daemon"]
      ++ lib.optional gradleHermetic "--offline";

    moduleSubdir = lib.removePrefix ":" gradleModule;

    commonAttrs =
      {
        inherit pname version;
        src = workspaceSrc;
        dontConfigure = true;
        strictDeps = true;

        nativeBuildInputs = [
          cargoNdk
          gradle
          jdk
          perl
          androidSdk
          rustToolchain
        ];

        ANDROID_NDK_HOME = ndkRoot;
        ANDROID_NDK_ROOT = ndkRoot;
        ANDROID_SDK_ROOT = "${androidSdk}/libexec/android-sdk";

        preBuild = ''
          export HOME="$NIX_BUILD_TOP/home"
          export CARGO_HOME="$NIX_BUILD_TOP/cargo-home"
          export CARGO_TARGET_DIR="$NIX_BUILD_TOP/cargo-target"
          export GRADLE_USER_HOME="$NIX_BUILD_TOP/.gradle-home"
          mkdir -p "$HOME" "$CARGO_HOME" "$CARGO_TARGET_DIR" "$GRADLE_USER_HOME"

          ${lib.optionalString cargoHermetic ''
            if [ ! -f ${lib.escapeShellArg "${cargoVendorDir}/config.toml"} ]; then
              echo "error: cargoVendorDir must contain config.toml from craneLib.vendorCargoDeps" >&2
              exit 1
            fi
            export CARGO_NET_OFFLINE=true
            cp ${lib.escapeShellArg "${cargoVendorDir}/config.toml"} "$CARGO_HOME/config.toml"
          ''}

          ${lib.optionalString gradleHermetic ''
            mkdir -p "$GRADLE_USER_HOME/caches/modules-2"
            tar -xf ${lib.escapeShellArg (toString mavenCacheTar)} -C "$GRADLE_USER_HOME/caches/modules-2"
            if [ ! -d "$GRADLE_USER_HOME/caches/modules-2/files-2.1" ]; then
              echo "error: mavenCacheTar must contain a top-level files-2.1 directory" >&2
              exit 1
            fi
            echo "[mkAndroidApk-hermetic] extracted Maven cache into $GRADLE_USER_HOME/caches/modules-2"
          ''}
        '';

        buildPhase = ''
          runHook preBuild

          echo "[mkAndroidApk] cargo ${lib.escapeShellArgs cargoArgs}"
          cargo ${lib.escapeShellArgs cargoArgs}

          echo "[mkAndroidApk] gradle ${lib.escapeShellArgs gradleArgs}"
          (cd ${lib.escapeShellArg androidDir} && gradle ${lib.escapeShellArgs gradleArgs})

          runHook postBuild
        '';

        installPhase = ''
          runHook preInstall

          mkdir -p "$out"
          if [ ! -f ${lib.escapeShellArg apkOutPath} ]; then
            echo "error: Gradle succeeded but APK was not found at ${apkOutPath}" >&2
            exit 1
          fi
          cp ${lib.escapeShellArg apkOutPath} "$out/"

          runHook postInstall
        '';

        meta = {
          description = "Android ${moduleSubdir} APK (${mode}, ${
            if hermetic
            then "hermetic"
            else "impure"
          })";
          platforms = ["x86_64-linux" "aarch64-linux"];
        };
      }
      // lib.optionalAttrs (cargoNdkPlatform != null) {
        CARGO_NDK_PLATFORM = toString cargoNdkPlatform;
      };
  in let
    package = pkgs.stdenv.mkDerivation (
      commonAttrs
      // lib.optionalAttrs (!hermetic) {
        # Cargo and/or Gradle still needs network access. The user must have
        # `sandbox = false` (or `relaxed`) in nix.conf for this to succeed.
        __noChroot = true;
      }
    );
  in
    package
    // {
      artifactBuilder = packageTests.mkArtifactBuilder {
        kind = "android-apk-builder";
        packageName = pname;
        inherit version buildCommand;
        output = toString package;
        inputs =
          [(toString workspaceSrc)]
          ++ lib.optional cargoHermetic (toString cargoVendorDir)
          ++ lib.optional gradleHermetic (toString mavenCacheTar);
        metadata = {
          inherit cargoPkg gradleModule jniLibsDir apkOutPath abi mode cargoFeatures cargoNoDefaultFeatures androidDir cargoNdkPlatform ndkVersion;
          inherit cargoHermetic gradleHermetic hermetic;
          helper = "mkAndroidApk";
        };
      };
    }
