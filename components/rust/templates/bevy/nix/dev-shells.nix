{
  pkgs,
  harbor-rs,
  toolchain,
  cross,
  cargoConfig,
  bevyDeps,
  checks ? {},
  extraPackages ? [],
  extraShellHook ? "",
}:
harbor-rs.lib.mkDevShells {
  inherit pkgs cross cargoConfig checks extraShellHook;
  inherit (toolchain) craneLib;
  pkgConfigDeps = bevyDeps.buildInputs;

  packages = bevyDeps.buildInputs ++ bevyDeps.nativeBuildInputs ++ extraPackages;

  extraEnv = {
    LD_LIBRARY_PATH = bevyDeps.ldLibraryPath;
  };
}
