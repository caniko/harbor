{
  pkgs,
  devShell,
}: let
  shells = {
    default = devShell.mkShell {
      inherit pkgs;
      # A build-only Bash must not shadow the interactive shell baseline.
      packages = [pkgs.bashNonInteractive];
    };
    custom = devShell.mkShell {
      inherit pkgs;
      packages = [pkgs.bashNonInteractive];
      # Language adapters such as Crane flatten spec.env into builder attrs.
      builder = spec:
        pkgs.mkShell (spec.env // {inherit (spec) packages shellHook;});
    };
  };
in
  pkgs.runCommand "harbor-meta-dev-shell-bash" {} ''
    set -euo pipefail
    ${pkgs.lib.concatMapStringsSep "\n" (name: ''
      echo "Checking ${name} shell"
      ${pkgs.python3}/bin/python3 ${../tests/test_dev_shell_bash.py} \
        --path ${pkgs.lib.escapeShellArg (pkgs.lib.makeBinPath shells.${name}.nativeBuildInputs)}
    '') (builtins.attrNames shells)}
    touch "$out"
  ''
