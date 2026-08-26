{
  nixpkgs,
  harbor-meta,
}: rec {
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

  mkEthDevShell = {pkgs, ...} @ args:
    harbor-meta.lib.devShell.mkShell {
      inherit pkgs;
      fragments = [
        (mkEthDevShellFragment args)
      ];
    };
}
