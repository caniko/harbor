# harbor-android

Reusable Android SDK, NDK, and APK helpers for Nix flakes.

- `mkAndroidSdk` — one `composeAndroidPackages` from caller `pkgs` (versions required)
- `mkAndroidDevShell` — `ANDROID_*` env + cargo-ndk / JDK / Gradle
- `mkAndroidApk` / `mkAndroidApkDevBuilder` / `mkAndroidFlavorTable` — Rust cdylib + Gradle APK
- `findLocalMavenCache` — optional host tarball pinned by a committed SHA-256
- `mkAndroidDeviceTools` — `android-device` enroll/verify for a local physical-device definition

`rustToolchain` is an argument (usually from `harbor-rs.lib.mkToolchain`). This flake does not take `harbor-rs` as an input.

```bash
nix flake init -t github:caniko/harbor-android
```
