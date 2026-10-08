# Android APK packages

rs-harbor exposes four Android helpers:

- `mkAndroidApk` builds one Rust `cdylib` and packages it with one Gradle module.
- `mkAndroidApkDevBuilder` creates an impure `nix run` helper for a checkout.
- `mkAndroidFlavorTable` expands a consumer-owned flavor table into packages,
  development builders, and apps.
- `findLocalMavenCache` imports an optional host-local Maven cache tarball by a
  committed SHA-256 file.

The consumer remains responsible for composing the Android SDK, declaring its
Gradle modules, and including the required Rust targets in `mkToolchain`:

```nix
toolchain = rs-harbor.lib.mkToolchain {
  inherit pkgs;
  crossTargets = [
    "aarch64-linux-android"
    "x86_64-linux-android"
  ];
};
```

## Flavor table

`mkAndroidFlavorTable` is the recommended interface when a project has more
than one APK or exposes both debug and release packages:

```nix
cargoVendorDir = toolchain.craneLib.vendorCargoDeps {
  src = workspaceSrc;
};

android = rs-harbor.lib.mkAndroidFlavorTable {
  inherit pkgs workspaceSrc cargoVendorDir;
  androidSdk = androidComposition.androidsdk;
  rustToolchain = toolchain.rustToolchain;
  cargoNdkPlatform = 28;
  mavenCacheTar = ./nix/android/gradle-cache.tar;

  commonCargoNoDefaultFeatures = true;
  flavors = {
    app = {
      cargoPkg = "my-game";
      gradleModule = ":app";
      cargoFeatures = ["tutorial"];
      packageModes = ["debug" "release"];
      packageAttr = mode: "android-apk-${mode}";
      devAppAttr = "android-apk";
    };
    test-peer = {
      cargoPkg = "my-game-test-peer";
      gradleModule = ":test-peer";
      packageModes = ["debug"];
      packageAttr = _mode: "android-test-peer-apk";
      devAppAttr = "android-test-peer-apk";
    };
  };
};

packages = android.packages;
apps = android.apps;
```

The table keeps Cargo features, `cargoNoDefaultFeatures`, and
`cargoNdkPlatform` attached to each flavor. Overriding `FLAVOR` on a
development app therefore cannot accidentally reuse another flavor's Cargo
flags. `apkOutPath` may be a string, an attrset with `debug` and `release`, or a
function from mode to path; mode-aware paths are required when one development
app supports both modes.

Set `cargoNdkPlatform` equal to the Gradle module's `minSdk`. The development
builder respects an explicit `CARGO_NDK_PLATFORM` environment override; the
derivation builder records the configured value directly.

## Hermetic inputs

A fully hermetic APK needs both dependency graphs:

1. `cargoVendorDir`, normally from `craneLib.vendorCargoDeps`, for Cargo.
2. `mavenCacheTar`, with a top-level `files-2.1/` directory, for Gradle.

Supplying `mavenCacheTar` without `cargoVendorDir` fails during evaluation. A
Maven-only cache cannot make `cargo ndk build` work in a sandbox, and treating
it as hermetic would hide a network dependency. In hermetic mode rs-harbor sets
`CARGO_NET_OFFLINE=true`, installs the vendor `config.toml` while preserving the
project's existing `.cargo/config.toml` through Cargo's normal configuration
hierarchy, extracts the Maven cache, and runs Gradle with `--offline`.

`gradleDeps` is reserved but not implemented. Passing a non-null value fails
during evaluation instead of producing a derivation that fails later. Use the
Maven cache path until a concrete gradle2nix manifest producer and schema are
available.

To create the Maven input, first populate a dedicated Gradle home with the
consumer's real assemble tasks, then pack exactly `files-2.1`:

```sh
GRADLE_USER_HOME="$PWD/android/.gradle-cache-android" \
  gradle :app:assembleRelease :test-peer:assembleDebug --no-daemon

tar --sort=name --mtime='2026-01-01 00:00:00 UTC' \
  --owner=0 --group=0 --numeric-owner \
  -cf nix/android/gradle-cache.tar \
  -C android/.gradle-cache-android/caches/modules-2 files-2.1

nix hash file --type sha256 nix/android/gradle-cache.tar \
  > nix/android/gradle-cache-sha256
```

The validation command is the consumer's real package output, for example:

```sh
nix build .#android-apk-release
```

`findLocalMavenCache` returns `null` when either file is absent. If the hash
file exists but is empty or invalid, evaluation fails because the committed
hash is a broken foundational input, not an optional cache miss.

## Impure development builder

`mkAndroidApkDevBuilder` requires `ANDROID_NDK_HOME`, `ANDROID_SDK_ROOT`,
`cargo`, `cargo-ndk`, and `gradle`. Select a build with environment variables:

```sh
FLAVOR=test-peer ABI=x86_64 MODE=debug nix run .#android-test-peer-apk
FLAVOR=app ABI=arm64-v8a MODE=release nix run .#android-apk
```

The helper fails if Gradle exits successfully but the declared APK path does
not exist. This catches stale flavor or mode wiring before a fix loop tries to
install the wrong artifact.

Both APK helpers attach `artifactBuilder` metadata. The flavor table supplies
exact `nix build .#...` and `nix run .#...` commands. Direct helper calls may
leave `buildCommand = null`; rs-harbor does not guess a downstream flake
attribute name.
