{
  pkgs,
  lib,
  self,
}: let
  templateFlake = builtins.readFile ../templates/default/flake.nix;
  fakeSdk = pkgs.runCommand "fake-androidsdk" {} ''
    mkdir -p $out/libexec/android-sdk/ndk/29.0.14206865
    mkdir -p $out/libexec/android-sdk/build-tools/34.0.0
    mkdir -p $out/libexec/android-sdk/platform-tools
    mkdir -p $out/libexec/android-sdk/emulator
    printf '#!/bin/sh\nexit 0\n' > $out/libexec/android-sdk/build-tools/34.0.0/aapt2
    chmod +x $out/libexec/android-sdk/build-tools/34.0.0/aapt2
  '';
  fakeAapt2 = "${fakeSdk}/libexec/android-sdk/build-tools/34.0.0/aapt2";
  fakeToolchain = pkgs.symlinkJoin {
    name = "fake-rust";
    paths = [pkgs.coreutils];
  };
in
  assert pkgs.lib.hasInfix "mkAndroidSdk" templateFlake;
  assert pkgs.lib.hasInfix "mkAndroidDevShell" templateFlake;
  assert pkgs.lib.hasInfix "harbor-android" templateFlake; {
    mkAndroidSdk-requires-versions = let
      missingPlatform = builtins.tryEval (lib.mkAndroidSdk {
        inherit pkgs;
        platformVersions = [];
        buildToolsVersions = ["34.0.0"];
        ndkVersions = ["29.0.14206865"];
      });
      missingBuildTools = builtins.tryEval (lib.mkAndroidSdk {
        inherit pkgs;
        platformVersions = ["34"];
        buildToolsVersions = [];
        ndkVersions = ["29.0.14206865"];
      });
      missingNdk = builtins.tryEval (lib.mkAndroidSdk {
        inherit pkgs;
        platformVersions = ["34"];
        buildToolsVersions = ["34.0.0"];
        ndkVersions = [];
      });
    in
      assert !missingPlatform.success;
      assert !missingBuildTools.success;
      assert !missingNdk.success;
        pkgs.runCommand "check-mkAndroidSdk-requires-versions" {} "touch $out";

    mkAndroidDevShell-exports = let
      shell = lib.mkAndroidDevShell {
        inherit pkgs;
        androidSdk = fakeSdk;
        ndkVersion = "29.0.14206865";
      };
    in
      assert pkgs.lib.hasInfix "ANDROID_NDK_HOME" shell.shellHook;
      assert pkgs.lib.hasInfix "ANDROID_SDK_ROOT" shell.shellHook;
      assert pkgs.lib.hasInfix "29.0.14206865" shell.shellHook;
      assert pkgs.lib.hasInfix "libexec/android-sdk" shell.shellHook;
        pkgs.runCommand "check-mkAndroidDevShell-exports" {} "touch $out";

    findLocalMavenCache-missing-inputs = let
      missingHash = lib.findLocalMavenCache {
        sha256Path = ../tests/fixtures/android-maven-cache/missing.sha256;
        hostPath = ../tests/fixtures/android-maven-cache/cache.tar;
        name = "fixture-cache.tar";
      };
      missingTar = lib.findLocalMavenCache {
        sha256Path = ../tests/fixtures/android-maven-cache/cache.sha256;
        hostPath = ../tests/fixtures/android-maven-cache/missing.tar;
        name = "fixture-cache.tar";
      };
    in
      assert missingHash == null;
      assert missingTar == null;
        pkgs.runCommand "check-findLocalMavenCache-missing-inputs" {} "touch $out";

    findLocalMavenCache-flat-hash = let
      cache = lib.findLocalMavenCache {
        sha256Path = ../tests/fixtures/android-maven-cache/cache.sha256;
        hostPath = ../tests/fixtures/android-maven-cache/cache.tar;
        name = "fixture-cache.tar";
      };
    in
      assert cache != null;
      assert pkgs.lib.hasSuffix "-fixture-cache.tar" (builtins.baseNameOf cache);
        pkgs.runCommand "check-findLocalMavenCache-flat-hash" {} "touch $out";

    findLocalMavenCache-rejects-invalid-hash = let
      result = builtins.tryEval (lib.findLocalMavenCache {
        sha256Path = ../tests/fixtures/android-maven-cache/invalid.sha256;
        hostPath = ../tests/fixtures/android-maven-cache/cache.tar;
        name = "fixture-cache.tar";
      });
    in
      assert !result.success;
        pkgs.runCommand "check-findLocalMavenCache-rejects-invalid-hash" {} "touch $out";

    mkAndroidFlavorTable-shape = let
      table = lib.mkAndroidFlavorTable {
        inherit pkgs;
        androidSdk = pkgs.emptyDirectory;
        rustToolchain = pkgs.emptyDirectory;
        workspaceSrc = pkgs.emptyDirectory;
        commonCargoFeatures = ["tutorial"];
        commonCargoNoDefaultFeatures = true;
        cargoNdkPlatform = 28;
        flavors = {
          app = {
            cargoPkg = "game";
            gradleModule = ":app";
            packageModes = ["debug" "release"];
            packageAttr = mode: "android-apk-${mode}";
            devAppAttr = "android-apk";
          };
          test-peer = {
            cargoPkg = "game-test-peer";
            gradleModule = ":test-peer";
            cargoFeatures = [];
            cargoNdkPlatform = 29;
            packageModes = ["debug"];
            packageAttr = mode: "android-test-peer-apk";
            devAppAttr = "android-test-peer-apk";
          };
        };
      };
    in
      assert table ? packages;
      assert table ? devBuilders;
      assert table ? apps;
      assert table.packages ? android-apk-debug;
      assert table.packages ? android-apk-release;
      assert table.packages ? android-test-peer-apk;
      assert !(table.packages ? android-test-peer-apk-release);
      assert table.devBuilders ? android-apk;
      assert table.devBuilders ? android-test-peer-apk;
      assert table.apps.android-apk.type == "app";
      assert table.apps.android-test-peer-apk.type == "app";
      assert table.packages.android-apk-debug.artifactBuilder.kind == "android-apk-builder";
      assert table.packages.android-apk-debug.artifactBuilder.buildCommand == "nix build .#android-apk-debug";
      assert table.packages.android-apk-debug.artifactBuilder.metadata.cargoNdkPlatform == 28;
      assert table.packages.android-test-peer-apk.artifactBuilder.buildCommand == "nix build .#android-test-peer-apk";
      assert table.packages.android-test-peer-apk.artifactBuilder.metadata.cargoFeatures == [];
      assert table.packages.android-test-peer-apk.artifactBuilder.metadata.cargoNdkPlatform == 29;
      assert table.devBuilders.android-apk.artifactBuilder.kind == "android-apk-dev-builder";
      assert table.devBuilders.android-apk.artifactBuilder.packageName == "android-apk";
      assert table.devBuilders.android-apk.artifactBuilder.buildCommand == "nix run .#android-apk";
      assert table.devBuilders.android-test-peer-apk.artifactBuilder.packageName == "android-test-peer-apk";
      assert table.devBuilders.android-test-peer-apk.artifactBuilder.buildCommand == "nix run .#android-test-peer-apk";
      assert table.devBuilders.android-test-peer-apk.artifactBuilder.metadata.cargoFeatures == [];
      assert table.devBuilders.android-test-peer-apk.artifactBuilder.metadata.cargoNdkPlatform == 29;
        pkgs.runCommand "check-mkAndroidFlavorTable-shape" {} "touch $out";

    mkAndroidApk-shape = let
      drv = lib.mkAndroidApk {
        inherit pkgs;
        androidSdk = fakeSdk;
        rustToolchain = fakeToolchain;
        workspaceSrc = ./.;
        cargoPkg = "test-pkg";
        gradleModule = ":app";
        jniLibsDir = "android/app/src/main/jniLibs";
        apkOutPath = "android/app/build/outputs/apk/debug/app-debug.apk";
        cargoNoDefaultFeatures = true;
        cargoFeatures = ["alpha" "beta"];
        cargoNdkPlatform = 28;
      };
    in
      assert drv.pname == "android-apk";
      assert drv ? artifactBuilder;
      assert drv.artifactBuilder.kind == "android-apk-builder";
      assert drv.artifactBuilder.output == toString drv;
      assert drv.artifactBuilder.buildCommand == null;
      assert drv.artifactBuilder.metadata.cargoHermetic == false;
      assert drv.artifactBuilder.metadata.gradleHermetic == false;
      assert drv.drvAttrs ? __noChroot;
      assert drv.drvAttrs.__noChroot == true;
      assert drv.drvAttrs.CARGO_NDK_PLATFORM == "28";
      assert builtins.elem pkgs.perl drv.drvAttrs.nativeBuildInputs;
      assert pkgs.lib.hasInfix "cargo ndk -t arm64-v8a" drv.drvAttrs.buildPhase;
      assert pkgs.lib.hasInfix "test-pkg" drv.drvAttrs.buildPhase;
      assert pkgs.lib.hasInfix "--no-default-features" drv.drvAttrs.buildPhase;
      assert pkgs.lib.hasInfix "--features alpha,beta" drv.drvAttrs.buildPhase;
      assert pkgs.lib.hasInfix ":app:assembleDebug" drv.drvAttrs.buildPhase;
      assert !(pkgs.lib.hasInfix "--offline" drv.drvAttrs.buildPhase);
      assert pkgs.lib.hasInfix "GRADLE_USER_HOME" drv.drvAttrs.preBuild;
      assert pkgs.lib.hasInfix "CARGO_HOME" drv.drvAttrs.preBuild;
      assert pkgs.lib.hasInfix "ANDROID_USER_HOME" drv.drvAttrs.preBuild;
        pkgs.runCommand "check-mkAndroidApk-shape" {} "touch $out";

    mkAndroidApk-hermetic = let
      fakeCacheTar = pkgs.runCommand "fake-cache.tar" {} ''
        mkdir -p $out
        touch $out/dummy
      '';
      fakeVendor = pkgs.runCommand "fake-cargo-vendor" {} ''
        mkdir -p $out
        touch $out/config.toml
      '';
      drv = lib.mkAndroidApk {
        inherit pkgs;
        androidSdk = fakeSdk;
        rustToolchain = fakeToolchain;
        workspaceSrc = ./.;
        cargoPkg = "test-peer";
        gradleModule = ":test-peer";
        jniLibsDir = "android/test-peer/src/main/jniLibs";
        apkOutPath = "android/test-peer/build/outputs/apk/release/test-peer-release.apk";
        mode = "release";
        mavenCacheTar = fakeCacheTar;
        cargoVendorDir = fakeVendor;
        aapt2 = fakeAapt2;
        buildCommand = "nix build .#android-test-peer-apk";
      };
    in
      assert !(drv.drvAttrs ? __noChroot && drv.drvAttrs.__noChroot == true);
      assert drv.artifactBuilder.buildCommand == "nix build .#android-test-peer-apk";
      assert drv.artifactBuilder.metadata.cargoHermetic == true;
      assert drv.artifactBuilder.metadata.gradleHermetic == true;
      assert drv.artifactBuilder.metadata.hermetic == true;
      assert builtins.length drv.artifactBuilder.inputs == 3;
      assert pkgs.lib.hasInfix "--offline" drv.drvAttrs.buildPhase;
      assert pkgs.lib.hasInfix ":test-peer:assembleRelease" drv.drvAttrs.buildPhase;
      assert pkgs.lib.hasInfix "--release" drv.drvAttrs.buildPhase;
      assert pkgs.lib.hasInfix "CARGO_NET_OFFLINE=true" drv.drvAttrs.preBuild;
      assert pkgs.lib.hasInfix "extracted Maven cache" drv.drvAttrs.preBuild;
        pkgs.runCommand "check-mkAndroidApk-hermetic" {} "touch $out";

    mkAndroidApk-hermetic-runtime = let
      fakeVendor = pkgs.runCommand "fake-cargo-vendor-runtime" {} ''
        mkdir -p $out
        cat > $out/config.toml <<EOF
        [source.crates-io]
        replace-with = "vendored-sources"
        [source.vendored-sources]
        directory = "$out"
        EOF
      '';
      fakeCacheTar = pkgs.runCommand "fake-maven-cache-runtime.tar" {nativeBuildInputs = [pkgs.gnutar];} ''
        mkdir -p cache/files-2.1/example/group
        touch cache/files-2.1/example/group/artifact.jar
        tar -cf $out -C cache files-2.1
      '';
      fakeCargo = pkgs.writeShellScriptBin "cargo" ''
        set -euo pipefail
        if [ "''${1:-}" = --version ]; then
          echo "cargo 1.95.0-fixture"
          exit 0
        fi
        test "$1" = ndk
        test "$CARGO_NET_OFFLINE" = true
        test "$CARGO_NDK_PLATFORM" = 28
        grep -q vendored-sources "$CARGO_HOME/config.toml"
        grep -q 'jobs = 1' .cargo/config.toml
        output=""
        while [ "$#" -gt 0 ]; do
          if [ "$1" = -o ]; then
            output="$2"
            shift 2
          else
            shift
          fi
        done
        test -n "$output"
        mkdir -p "$output/arm64-v8a"
        touch "$output/arm64-v8a/libfixture.so"
      '';
      fakeGradle = pkgs.writeShellScriptBin "gradle" ''
        set -euo pipefail
        test "$1" = :app:assembleDebug
        test "$2" = --no-daemon
        test "$3" = --offline
        test -d "$GRADLE_USER_HOME/caches/modules-2/files-2.1"
        mkdir -p app/build/outputs/apk/debug
        printf 'fixture apk' > app/build/outputs/apk/debug/app-debug.apk
      '';
      fakeRustc = pkgs.writeShellScriptBin "rustc" ''
        test "$1" = --version
        echo "rustc 1.95.0-fixture"
      '';
      fakeRuntimeToolchain = pkgs.symlinkJoin {
        name = "fake-runtime-rust";
        paths = [fakeCargo fakeRustc];
      };
      package = lib.mkAndroidApk {
        inherit pkgs;
        androidSdk = fakeSdk;
        rustToolchain = fakeRuntimeToolchain;
        cargoNdk = pkgs.emptyDirectory;
        jdk = pkgs.emptyDirectory;
        gradle = fakeGradle;
        workspaceSrc = ../tests/fixtures/android-workspace;
        cargoVendorDir = fakeVendor;
        mavenCacheTar = fakeCacheTar;
        cargoPkg = "fixture";
        gradleModule = ":app";
        jniLibsDir = "android/app/src/main/jniLibs";
        apkOutPath = "android/app/build/outputs/apk/debug/app-debug.apk";
        cargoNdkPlatform = 28;
        aapt2 = fakeAapt2;
        sourceIdentity = {
          commit = "0123456789abcdef";
          workspaceDigest = "sha256-fixture";
          dirty = false;
        };
        cargoLock = ../tests/fixtures/android-workspace/Cargo.lock;
        flakeLock = ../tests/fixtures/android-workspace/flake.lock;
      };
    in
      package.overrideAttrs (old: {
        postFixup =
          (old.postFixup or "")
          + ''
            unsigned="$out/unsigned-manifest.json"
            distribution="$out/distribution-manifest.json"
            test -f "$unsigned"
            test -f "$distribution"
            apk_digest="$(${pkgs.coreutils}/bin/sha256sum "$out/app-debug.apk")"
            apk_digest="''${apk_digest%% *}"
            test "$(${pkgs.jq}/bin/jq -r '.entries[0].digest' "$unsigned")" = "$apk_digest"
            test "$(${pkgs.jq}/bin/jq -r '.entries[0].role' "$unsigned")" = Package
            unsigned_digest="$(${pkgs.jq}/bin/jq -r '.digest' "$unsigned")"
            calculated="$(${pkgs.jq}/bin/jq 'del(.digest)' -cSj "$unsigned" | ${pkgs.coreutils}/bin/sha256sum)"
            calculated="''${calculated%% *}"
            test "$unsigned_digest" = "$calculated"
            test "$(${pkgs.jq}/bin/jq -r '.unsigned_digest' "$distribution")" = "$unsigned_digest"
            test "$(${pkgs.jq}/bin/jq -r '.package_digest' "$distribution")" = "$apk_digest"
            test "$(${pkgs.jq}/bin/jq -r '.signed' "$distribution")" = false
          '';
      });

    mkAndroidApk-rejects-bad-mode = let
      result = builtins.tryEval (lib.mkAndroidApk {
        inherit pkgs;
        androidSdk = fakeSdk;
        rustToolchain = fakeToolchain;
        workspaceSrc = ./.;
        cargoPkg = "x";
        gradleModule = ":x";
        jniLibsDir = "x";
        apkOutPath = "x";
        mode = "bogus";
      });
    in
      assert !result.success;
        pkgs.runCommand "check-mkAndroidApk-rejects-bad-mode" {} "touch $out";

    mkAndroidApk-rejects-missing-toolchain = let
      result = builtins.tryEval (lib.mkAndroidApk {
        inherit pkgs;
        androidSdk = fakeSdk;
        rustToolchain = null;
        workspaceSrc = ./.;
        cargoPkg = "x";
        gradleModule = ":x";
        jniLibsDir = "x";
        apkOutPath = "x";
      });
    in
      assert !result.success;
        pkgs.runCommand "check-mkAndroidApk-rejects-missing-toolchain" {} "touch $out";

    mkAndroidApk-rejects-maven-without-cargo-vendor = let
      result = builtins.tryEval (lib.mkAndroidApk {
        inherit pkgs;
        androidSdk = fakeSdk;
        rustToolchain = fakeToolchain;
        workspaceSrc = ./.;
        cargoPkg = "x";
        gradleModule = ":x";
        jniLibsDir = "x";
        apkOutPath = "x";
        mavenCacheTar = ../tests/fixtures/android-maven-cache/cache.tar;
      });
    in
      assert !result.success;
        pkgs.runCommand "check-mkAndroidApk-rejects-maven-without-cargo-vendor" {} "touch $out";

    mkAndroidApk-rejects-unimplemented-gradle-deps = let
      result = builtins.tryEval (lib.mkAndroidApk {
        inherit pkgs;
        androidSdk = fakeSdk;
        rustToolchain = fakeToolchain;
        workspaceSrc = ./.;
        cargoPkg = "x";
        gradleModule = ":x";
        jniLibsDir = "x";
        apkOutPath = "x";
        gradleDeps = ../tests/fixtures/android-maven-cache/cache.tar;
      });
    in
      assert !result.success;
        pkgs.runCommand "check-mkAndroidApk-rejects-unimplemented-gradle-deps" {} "touch $out";

    mkAndroidApk-rejects-incomplete-source-identity = let
      result = builtins.tryEval (lib.mkAndroidApk {
        inherit pkgs;
        androidSdk = fakeSdk;
        rustToolchain = fakeToolchain;
        workspaceSrc = ./.;
        cargoPkg = "x";
        gradleModule = ":x";
        jniLibsDir = "x";
        apkOutPath = "x";
        sourceIdentity = {commit = "abc";};
      });
    in
      assert !result.success;
        pkgs.runCommand "check-mkAndroidApk-rejects-incomplete-source-identity" {} "touch $out";

    mkAndroidApk-rejects-hermetic-build-without-aapt2 = let
      fakeCacheTar = pkgs.runCommand "fake-cache-without-aapt2.tar" {} ''
        mkdir -p $out
        touch $out/dummy
      '';
      fakeVendor = pkgs.runCommand "fake-vendor-without-aapt2" {} ''
        mkdir -p $out
        touch $out/config.toml
      '';
      result = builtins.tryEval (lib.mkAndroidApk {
        inherit pkgs;
        androidSdk = fakeSdk;
        rustToolchain = fakeToolchain;
        workspaceSrc = ./.;
        cargoPkg = "x";
        gradleModule = ":x";
        jniLibsDir = "x";
        apkOutPath = "x";
        mavenCacheTar = fakeCacheTar;
        cargoVendorDir = fakeVendor;
      });
    in
      assert !result.success;
        pkgs.runCommand "check-mkAndroidApk-rejects-hermetic-build-without-aapt2" {} "touch $out";

    mkAndroidApkDevBuilder-shape = let
      script = lib.mkAndroidApkDevBuilder {
        inherit pkgs;
        defaultFlavor = "test-peer";
        cargoNoDefaultFeatures = true;
        cargoFeatures = ["tutorial"];
        cargoNdkPlatform = 28;
        flavors = {
          app = {
            cargoPkg = "game";
            gradleModule = ":app";
            jniLibsDir = "android/app/src/main/jniLibs";
            apkOutPath = mode: "android/app/build/outputs/apk/${mode}/app-${mode}.apk";
          };
          test-peer = {
            cargoPkg = "game-android-test-peer";
            gradleModule = ":test-peer";
            jniLibsDir = "android/test-peer/src/main/jniLibs";
            apkOutPath = {
              debug = "android/test-peer/build/outputs/apk/debug/test-peer-debug.apk";
              release = "android/test-peer/build/outputs/apk/release/test-peer-release.apk";
            };
            cargoFeatures = ["peer"];
            cargoNoDefaultFeatures = false;
            cargoNdkPlatform = 29;
          };
        };
      };
    in
      assert script ? artifactBuilder;
      assert script.artifactBuilder.kind == "android-apk-dev-builder";
      assert script.artifactBuilder.output == toString script;
      assert script.artifactBuilder.buildCommand == null;
        pkgs.runCommand "check-mkAndroidApkDevBuilder-shape" {} ''
          cp ${script} script
          grep 'cargo_args=(ndk -t "$abi" -o "$jni_libs_dir" build)' script
          grep 'game-android-test-peer' script
          grep 'gradle_module=:test-peer' script
          grep 'gradle "$gradle_module:assemble$mode_cap"' script
          grep 'cargo_features=peer' script
          grep 'cargo_no_default=0' script
          grep 'cargo_ndk_platform=29' script
          grep 'apk_out_path_release=android/test-peer/build/outputs/apk/release/test-peer-release.apk' script
          touch $out
        '';

    mkAndroidApkDevBuilder-runtime = let
      fakeCargo = pkgs.writeShellScriptBin "cargo" ''
        set -euo pipefail
        printf '%s\n%s\n' "$CARGO_NDK_PLATFORM" "$*" > "$TRACE_DIR/cargo-$TRACE_CASE"
        output=""
        while [ "$#" -gt 0 ]; do
          if [ "$1" = -o ]; then
            output="$2"
            shift 2
          else
            shift
          fi
        done
        test -n "$output"
        mkdir -p "$output"
        touch "$output/libfixture.so"
      '';
      fakeCargoNdk = pkgs.writeShellScriptBin "cargo-ndk" "exit 0";
      fakeGradle = pkgs.writeShellScriptBin "gradle" ''
        set -euo pipefail
        printf '%s\n' "$*" > "$TRACE_DIR/gradle-$TRACE_CASE"
        case "$1" in
          :app:assembleRelease)
            mkdir -p app/build/outputs/apk/release
            touch app/build/outputs/apk/release/app-release.apk
            ;;
          :test-peer:assembleDebug)
            mkdir -p test-peer/build/outputs/apk/debug
            touch test-peer/build/outputs/apk/debug/test-peer-debug.apk
            ;;
          *)
            exit 1
            ;;
        esac
      '';
      script = lib.mkAndroidApkDevBuilder {
        inherit pkgs;
        defaultFlavor = "test-peer";
        cargoNoDefaultFeatures = true;
        cargoFeatures = ["common"];
        cargoNdkPlatform = 28;
        flavors = {
          app = {
            cargoPkg = "game";
            gradleModule = ":app";
            jniLibsDir = "android/app/src/main/jniLibs";
            apkOutPath = mode: "android/app/build/outputs/apk/${mode}/app-${mode}.apk";
            cargoFeatures = ["app-feature"];
          };
          test-peer = {
            cargoPkg = "game-android-test-peer";
            gradleModule = ":test-peer";
            jniLibsDir = "android/test-peer/src/main/jniLibs";
            apkOutPath = mode: "android/test-peer/build/outputs/apk/${mode}/test-peer-${mode}.apk";
            cargoFeatures = ["peer-feature"];
            cargoNoDefaultFeatures = false;
            cargoNdkPlatform = 29;
          };
        };
      };
    in
      pkgs.runCommand "check-mkAndroidApkDevBuilder-runtime" {
        nativeBuildInputs = [fakeCargo fakeCargoNdk fakeGradle];
      } ''
        export HOME="$TMPDIR/home"
        mkdir -p "$HOME"
        export ANDROID_NDK_HOME="$PWD/ndk"
        export ANDROID_SDK_ROOT="$PWD/sdk"
        export TRACE_DIR="$PWD/trace"
        mkdir -p "$ANDROID_NDK_HOME" "$ANDROID_SDK_ROOT" "$TRACE_DIR" work/android/app work/android/test-peer
        cd work

        TRACE_CASE=test-peer ${script}
        test -f android/test-peer/build/outputs/apk/debug/test-peer-debug.apk
        read -r test_peer_platform < "$TRACE_DIR/cargo-test-peer"
        test "$test_peer_platform" = 29
        grep -q -- '--features peer-feature' "$TRACE_DIR/cargo-test-peer"
        if grep -q -- '--no-default-features' "$TRACE_DIR/cargo-test-peer"; then
          exit 1
        fi
        grep -q ':test-peer:assembleDebug' "$TRACE_DIR/gradle-test-peer"

        TRACE_CASE=app FLAVOR=app ABI=x86_64 MODE=release ${script}
        test -f android/app/build/outputs/apk/release/app-release.apk
        read -r app_platform < "$TRACE_DIR/cargo-app"
        test "$app_platform" = 28
        grep -q -- '-t x86_64' "$TRACE_DIR/cargo-app"
        grep -q -- '--release' "$TRACE_DIR/cargo-app"
        grep -q -- '--no-default-features' "$TRACE_DIR/cargo-app"
        grep -q -- '--features app-feature' "$TRACE_DIR/cargo-app"
        grep -q ':app:assembleRelease' "$TRACE_DIR/gradle-app"

        touch $out
      '';

    mkAndroidDeviceTools-runtime = let
      tools = lib.mkAndroidDeviceTools {inherit pkgs;};
      fakeAdb = pkgs.writeShellScriptBin "adb" ''
        case "$*" in
          "devices -l")
            printf 'List of devices attached\n%s\n' "$FAKE_DEVICES"
            ;;
          *"-s S1 shell getprop ro.product.device") echo "mustang" ;;
          *"-s S1 shell getprop ro.product.model") echo "Pixel 10 Pro XL" ;;
          *"-s S2 shell getprop ro.product.device") echo "other" ;;
          *"-s S2 shell getprop ro.product.model") echo "Other" ;;
          *) echo "unexpected adb call: $*" >&2; exit 2 ;;
        esac
      '';
    in
      pkgs.runCommand "check-mkAndroidDeviceTools-runtime" {
        nativeBuildInputs = [pkgs.jq];
      } ''
        dev=${tools}/bin/android-device
        export PATH=${fakeAdb}/bin:$PATH

        usb_line="S1 device usb:9-3 product:mustang model:Pixel_10_Pro_XL device:mustang transport_id:1"
        export FAKE_DEVICES="$usb_line"

        # Enroll writes 0600 JSON and refuses to overwrite.
        $dev enroll --serial S1 --out def.json --product mustang --adb adb
        test "$(jq -r .schemaVersion def.json)" = 1
        test "$(jq -r .adbSerial def.json)" = S1
        test "$(jq -r .product def.json)" = mustang
        test "$(stat -c %a def.json)" = 600
        if $dev enroll --serial S1 --out def.json --adb adb 2>err; then exit 1; fi
        grep -q 'exists' err

        # Verify prints the serial for capture.
        test "$($dev verify --definition def.json --adb adb)" = S1

        # Wrong product in the definition fails closed.
        jq '.product = "other"' def.json > bad.json
        if $dev verify --definition bad.json --adb adb 2>err; then exit 1; fi
        grep -q 'does not match' err

        # Non-string definition fields fail closed.
        jq '.adbSerial = 10' def.json > num.json
        if $dev verify --definition num.json --adb adb 2>err; then exit 1; fi
        grep -q 'not a valid device definition' err

        # Offline serial fails closed.
        export FAKE_DEVICES="S1 offline usb:9-3 product:mustang model:Pixel_10_Pro_XL device:mustang transport_id:1"
        if $dev verify --definition def.json --adb adb 2>err; then exit 1; fi
        grep -q 'not authorized/online' err

        # Unauthorized serial fails closed with its exact state.
        export FAKE_DEVICES="S1 unauthorized usb:9-3 product:mustang model:Pixel_10_Pro_XL device:mustang transport_id:1"
        if $dev verify --definition def.json --adb adb 2>err; then exit 1; fi
        grep -q 'unauthorized' err

        # Non-USB transport fails closed.
        export FAKE_DEVICES="S1 device product:mustang model:Pixel_10_Pro_XL device:mustang transport_id:1"
        if $dev verify --definition def.json --adb adb 2>err; then exit 1; fi
        grep -q 'not a USB device' err

        # Ambiguous selection fails closed.
        export FAKE_DEVICES="$usb_line
S9 device usb:9-4 product:mustang model:Pixel_10_Pro_XL device:mustang transport_id:9"
        if $dev enroll --serial NOMATCH --out def2.json --adb adb 2>err; then exit 1; fi
        grep -q 'not uniquely connected' err

        touch $out
      '';

    mkAndroidDeviceTools-rejects-bad-input = let
      result = builtins.tryEval (lib.mkAndroidDeviceTools {
        inherit pkgs;
        adbPackage = "not-a-package";
      });
    in
      assert !result.success;
        pkgs.runCommand "check-mkAndroidDeviceTools-rejects-bad-input" {} "touch $out";

    lib-exports = let
      names = [
        "mkAndroidSdk"
        "mkAndroidDevShell"
        "findLocalMavenCache"
        "mkAndroidApk"
        "mkAndroidApkDevBuilder"
        "mkAndroidFlavorTable"
        "mkAndroidDeviceTools"
      ];
    in
      assert builtins.all (name: self.lib ? ${name}) names;
        pkgs.runCommand "check-lib-exports" {} "touch $out";
  }
