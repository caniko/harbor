{
  nixpkgs,
  sccacheDefault,
}: let
  lib = nixpkgs.lib;
in {
  inherit sccacheDefault;

  mkUserDaemonEnv = import ./mkUserDaemonEnv.nix {inherit lib sccacheDefault;};

  mkClientWrapper = {
    pkgs,
    sccachePackage,
    serviceName ? "sccache-user-daemon.service",
  }:
    import ./client-wrapper.nix {
      inherit pkgs sccacheDefault sccachePackage serviceName;
    };
}
