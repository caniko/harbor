{self}: {
  config,
  lib,
  options,
  ...
}: let
  cfg = config.programs.harbor;
  available = lib.hasAttrByPath ["fleetix" "mcp" "servers"] options;
in {
  key = "harbor.homeManagerModules.fleetix";
  imports = [self.homeManagerModules.default];
  config = lib.mkMerge [
    {
      assertions = [
        {
          assertion = available;
          message = "harbor: import your Fleetix homeModules.mcp alongside Harbor's fleetix adapter";
        }
      ];
    }
    (lib.optionalAttrs available {fleetix.mcp.servers = lib.mkIf cfg.enable cfg.mcp.servers;})
  ];
}
