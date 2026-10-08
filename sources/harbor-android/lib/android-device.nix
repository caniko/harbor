# mkAndroidDeviceTools :: { pkgs, adbPackage?, jq? } -> package
#
# Physical-device enrollment and verification helper for dev-shell and
# device-oriented smoke loops. Packages `android-device` (canonical source in
# ./android-device.sh) with `jq`; `adb` resolves at runtime from `ADB` or
# `PATH` so the same binary works with any platform-tools.
#
# Device definitions are local runtime JSON files, never flake inputs: no
# serial number enters the Nix store through this helper. Authorization
# (which user/profile a caller may touch) stays with the caller.
{
  pkgs,
  adbPackage ? null,
  jq ? pkgs.jq,
  lib ? pkgs.lib,
}: let
  validPackage = value:
    value == null
    || (builtins.isAttrs value
      && builtins.hasAttr "type" value
      && value.type == "derivation");
in
  assert lib.assertMsg (validPackage adbPackage)
  "harbor-android: mkAndroidDeviceTools `adbPackage` must be null or a package";
  assert lib.assertMsg (validPackage jq)
  "harbor-android: mkAndroidDeviceTools `jq` must be a package";
    pkgs.writeShellApplication {
      name = "android-device";
      runtimeInputs = [jq] ++ lib.optional (adbPackage != null) adbPackage;
      text = builtins.readFile ./android-device.sh;
    }
