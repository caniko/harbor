{self}: {
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.services.harbor-cad;
in {
  options.services.harbor-cad.enable = lib.mkEnableOption "harbor-cad user worker package (activation remains operator-owned)";
  config = lib.mkIf cfg.enable {environment.systemPackages = [self.packages.${pkgs.stdenv.hostPlatform.system}.default];};
}
