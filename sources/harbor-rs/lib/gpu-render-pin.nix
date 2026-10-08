{
  pkgs,
  profile ? "mesa-radv",
  expected ? {},
}: let
  lib = pkgs.lib;

  icdBySystem = {
    x86_64-linux = "radeon_icd.x86_64.json";
    i686-linux = "radeon_icd.i686.json";
    aarch64-linux = "radeon_icd.aarch64.json";
  };

  system = pkgs.stdenv.hostPlatform.system;

  mesaRadvIcd =
    if builtins.hasAttr system icdBySystem
    then "${pkgs.mesa}/share/vulkan/icd.d/${icdBySystem.${system}}"
    else throw "harbor-rs mkGpuRenderPin mesa-radv only supports Linux systems with a known RADV ICD name";

  profiles = {
    mesa-radv = {
      env = {
        RS_HARBOR_GPU_PIN_PROFILE = "mesa-radv";
        RS_HARBOR_GPU_EXPECTED_BACKEND = "Vulkan";
        RS_HARBOR_GPU_EXPECTED_VENDOR = "0x1002";
        RS_HARBOR_GPU_EXPECTED_DRIVER_CONTAINS = "radv";
        WGPU_BACKEND = "vulkan";
        VK_DRIVER_FILES = mesaRadvIcd;
      };
      packages = [
        pkgs.mesa
        pkgs.vulkan-loader
      ];
    };
  };

  base =
    profiles.${profile}
      or (throw "unknown harbor-rs GPU render pin profile '${profile}'");
in
  base
  // {
    env = base.env // lib.filterAttrs (_: value: value != null) expected;
  }
