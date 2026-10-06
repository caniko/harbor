{harbor-meta}: rec {
  timezone = harbor-meta.lib.timezone;
  mkEthToolchain = {
    pkgs,
    foundry ? pkgs.foundry,
    solc ? pkgs.solc,
  }: {
    packages = [foundry solc];
    env = {
      FOUNDRY_SOLC = "${solc}/bin/solc";
      FOUNDRY_VERSION = foundry.version;
      SOLC_VERSION = solc.version;
    };
  };

  mkEthDevShellFragment = args: let
    toolchain = mkEthToolchain args;
  in {
    inherit (toolchain) packages env;
    shellHook = "";
  };

  mkEthDevShell = {
    pkgs,
    timeZone ? "UTC",
    ...
  } @ args:
    harbor-meta.lib.devShell.mkShell {
      inherit pkgs timeZone;
      fragments = [
        (mkEthDevShellFragment (builtins.removeAttrs args ["timeZone"]))
      ];
    };
}
