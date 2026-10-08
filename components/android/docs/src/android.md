# Android helpers

harbor-android owns Android SDK composition, the cargo-ndk/Gradle dev shell,
Rust+Gradle APK builders, flavor expansion, and pinned Maven-cache discovery.

The consumer still owns flavor names, Cargo features, Gradle modules, API/NDK
pins, Android application manifests, signing, and package IDs. Pass `rustToolchain` from the
workspace (typically `harbor-rs.lib.mkToolchain`); do not add `harbor-rs` as an
input of this flake.

```nix
androidSdk = harbor-android.lib.mkAndroidSdk {
  inherit pkgs;
  platformVersions = ["34"];
  buildToolsVersions = ["34.0.0"];
  ndkVersions = ["29.0.14206865"];
};

devShells.android = harbor-android.lib.mkAndroidDevShell {
  inherit pkgs androidSdk;
  ndkVersion = "29.0.14206865";
  rustToolchain = toolchain.rustToolchain;
};
```

## Flavor table

```nix
android = harbor-android.lib.mkAndroidFlavorTable {
  inherit pkgs workspaceSrc cargoVendorDir androidSdk;
  rustToolchain = toolchain.rustToolchain;
  cargoNdkPlatform = 28;
  mavenCacheTar = ./nix/android/gradle-cache.tar;
  flavors.app = {
    cargoPkg = "my-game";
    gradleModule = ":app";
    packageModes = ["debug" "release"];
  };
};
```

## Maven cache contract

A hermetic APK needs both graphs: `cargoVendorDir` from
`craneLib.vendorCargoDeps`, and `mavenCacheTar` whose archive contains a
top-level `files-2.1/` directory. Extra siblings such as `metadata-2.107` are
allowed; the builder only requires `files-2.1`.

```sh
tar --sort=name --mtime='2026-01-01 00:00:00 UTC' \
  --owner=0 --group=0 --numeric-owner \
  -cf nix/android/gradle-cache.tar \
  -C android/.gradle-cache-android/caches/modules-2 files-2.1
```

`findLocalMavenCache` returns `null` when either the hash file or tarball is
absent. An empty or invalid committed hash fails evaluation.

## Physical test devices

`mkAndroidDeviceTools` packages `android-device`, a small enroll/verify
helper for dev-shell and device-smoke loops. Definitions are local runtime
JSON (`{schemaVersion, adbSerial, product, model}`), never flake inputs, so
no serial enters the Nix store. Enrollment requires one uniquely connected,
authorized USB device and refuses to overwrite; verification rechecks the
serial, USB transport, and product on every run and prints the serial for
capture. Which user/profile a caller may touch stays with the caller.

```sh
android-device enroll --serial "$SERIAL" --product mustang --out .android-device.local.json
serial="$(android-device verify --definition .android-device.local.json)"
```

## Package identity

`mkAndroidApk` can emit package-identity sidecars when the caller supplies
`sourceIdentity`, `cargoLock`, and `flakeLock`. Harbor records the actual APK,
ABI, mode, features, toolchain, SDK, and hermeticity. `signed` means signed by a
release authority; APK builders always emit `false` even when Gradle used a
debug keystore.

```nix
sourceIdentity = {
  commit = self.rev or self.dirtyRev;
  workspaceDigest = self.narHash;
  dirty = !(self ? rev);
};
```

For offline Gradle builds, pass the SDK's `aapt2` executable. Harbor preserves
existing `GRADLE_OPTS` while setting `aapt2FromMavenOverride`.
