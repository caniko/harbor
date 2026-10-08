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
  checks = builtins.mapAttrs (name: shell:
    pkgs.runCommand "harbor-meta-dev-shell-bash-${name}" {
      inherit (shell) nativeBuildInputs;
    } ''
      set -euo pipefail
      ${pkgs.python3}/bin/python3 ${../tests/test_dev_shell_bash.py}
      touch "$out"
    '')
  shells;
in
  pkgs.linkFarm "harbor-meta-dev-shell-bash" (pkgs.lib.mapAttrsToList (name: path: {inherit name path;}) checks)
