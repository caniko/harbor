# mkAndroidSdk :: {
#   pkgs, platformVersions, buildToolsVersions, ndkVersions,
#   includeNDK?, includeEmulator?, includeSystemImages?,
#   systemImageTypes?, abiVersions?, extraComposeArgs?,
# } -> androidenv composition
#
# Compose one Android SDK/NDK from the caller's nixpkgs. Requires the three
# version lists; never imports nixpkgs on its own. Reimports `pkgs.path` with
# `allowUnfree` and `android_sdk.accept_license` so consumers do not have to
# thread those flags through every flake pkgs instance.
{
  pkgs,
  platformVersions,
  buildToolsVersions,
  ndkVersions,
  includeNDK ? true,
  includeEmulator ? false,
  includeSystemImages ? false,
  systemImageTypes ? ["google_apis"],
  abiVersions ? ["arm64-v8a"],
  extraComposeArgs ? {},
}: let
  lib = pkgs.lib;
in
  assert lib.assertMsg (builtins.isList platformVersions && platformVersions != [])
  "harbor-android: mkAndroidSdk requires a non-empty `platformVersions` list";
  assert lib.assertMsg (builtins.isList buildToolsVersions && buildToolsVersions != [])
  "harbor-android: mkAndroidSdk requires a non-empty `buildToolsVersions` list";
  assert lib.assertMsg (builtins.isList ndkVersions && ndkVersions != [])
  "harbor-android: mkAndroidSdk requires a non-empty `ndkVersions` list";
  assert lib.assertMsg (builtins.isAttrs extraComposeArgs)
  "harbor-android: mkAndroidSdk `extraComposeArgs` must be an attrset"; let
    androidPkgs = import pkgs.path {
      inherit (pkgs.stdenv.hostPlatform) system;
      config =
        (pkgs.config or {})
        // {
          allowUnfree = true;
          android_sdk.accept_license = true;
        };
    };
  in
    androidPkgs.androidenv.composeAndroidPackages (
      {
        inherit
          platformVersions
          buildToolsVersions
          includeNDK
          includeEmulator
          includeSystemImages
          systemImageTypes
          abiVersions
          ndkVersions
          ;
      }
      // extraComposeArgs
    )
