{self}: {
  config,
  lib,
  options,
  ...
}: let
  cfg = config.programs.harbor;
  available = lib.hasAttrByPath ["programs" "skillnet" "externalManifests"] options;
in {
  key = "harbor.homeManagerModules.skillnet";
  imports = [self.homeManagerModules.default];
  config = lib.mkMerge [
    {
      assertions = [
        {
          assertion = available;
          message = "harbor: import your Skillnet module alongside Harbor's skillnet adapter";
        }
      ];
    }
    (lib.optionalAttrs available {programs.skillnet.externalManifests = lib.mkIf (cfg.enable && cfg.skills != []) (lib.mkAfter ["${cfg.skillBundle}/Skillnet.pkl"]);})
  ];
}
