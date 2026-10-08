# mkAndroidDevShell :: {
#   pkgs, androidSdk, ndkVersion,
#   rustToolchain?, extraPackages?, extraShellHook?, base?,
# } -> derivation
#
# Android env for `cargo ndk` + Gradle. Pass `base` to overlay an existing
# rust/dev shell; omit it for a standalone mkShell. Does not compose an SDK.
{
  pkgs,
  androidSdk,
  ndkVersion,
  rustToolchain ? null,
  extraPackages ? [],
  extraShellHook ? "",
  base ? null,
}: let
  lib = pkgs.lib;
in
  assert lib.assertMsg (androidSdk != null)
  "harbor-android: mkAndroidDevShell requires `androidSdk` from mkAndroidSdk";
  assert lib.assertMsg (builtins.isString ndkVersion && ndkVersion != "")
  "harbor-android: mkAndroidDevShell requires a non-empty `ndkVersion`";
  assert lib.assertMsg (builtins.isList extraPackages)
  "harbor-android: mkAndroidDevShell `extraPackages` must be a list";
  assert lib.assertMsg (builtins.isString extraShellHook)
  "harbor-android: mkAndroidDevShell `extraShellHook` must be a string"; let
    ndkRoot = "${androidSdk}/libexec/android-sdk/ndk/${ndkVersion}";
    sdkRoot = "${androidSdk}/libexec/android-sdk";
    packages =
      [
        pkgs.cargo-ndk
        androidSdk
        pkgs.jdk21
        pkgs.gradle
      ]
      ++ lib.optional (rustToolchain != null) rustToolchain
      ++ extraPackages;
    hook = ''
      export ANDROID_NDK_HOME="${ndkRoot}"
      export ANDROID_NDK_ROOT="$ANDROID_NDK_HOME"
      export ANDROID_SDK_ROOT="${sdkRoot}"
      export ANDROID_AVD_HOME="''${ANDROID_AVD_HOME:-$HOME/.config/.android/avd}"
      echo "[android] ANDROID_NDK_HOME=$ANDROID_NDK_HOME"
    ''
    + extraShellHook;
  in
    if base != null
    then
      base.overrideAttrs (old: {
        buildInputs = (old.buildInputs or []) ++ packages;
        shellHook = (old.shellHook or "") + hook;
      })
    else
      pkgs.mkShell {
        inherit packages;
        shellHook = hook;
      }
