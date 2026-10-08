{pkgs}: let
  inherit (pkgs) lib;

  # Bevy/wgpu and winit load these at runtime via dlopen;
  # they must be on LD_LIBRARY_PATH for NixOS.
  runtimeLibs = with pkgs; [
    vulkan-loader
    wayland
    libxkbcommon

    # X11 runtime
    xorg.libX11
    xorg.libXcursor
    xorg.libXi
    xorg.libXrandr
  ];
in {
  # Build-time native dependencies (linked by build scripts)
  buildInputs = with pkgs; [
    # Graphics — Vulkan
    vulkan-loader
    vulkan-headers
    vulkan-validation-layers

    # Windowing — Wayland
    wayland
    libxkbcommon

    # Windowing — X11
    xorg.libX11
    xorg.libXcursor
    xorg.libXi
    xorg.libXrandr

    # Audio (rodio/ALSA backend)
    alsa-lib

    # Input/device hotplug
    udev
  ];

  # Packages needed at build time (compilers, pkg-config wrappers)
  nativeBuildInputs = with pkgs; [
    pkg-config
    clang
  ];

  # LD_LIBRARY_PATH for runtime dynamic linking
  ldLibraryPath = lib.makeLibraryPath runtimeLibs;
}
