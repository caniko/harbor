{
  pkgs,
  lib,
  ...
}: {
  settings.formatter.forge = {
    command = lib.mkDefault "${pkgs.foundry}/bin/forge";
    options = ["fmt"];
    includes = ["*.sol"];
  };
}
