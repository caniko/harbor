# Loader and executable paths belong to the launched command, not its shell.
{
  pkgs,
  name ? "harbor-runtime",
  libraryPackages ? [],
  pathPackages ? [],
  env ? {},
  defaultEnv ? {},
}: let
  inherit (pkgs) lib;
  validName = key: builtins.match "[A-Za-z_][A-Za-z0-9_]*" key != null;
  defaults =
    lib.mapAttrsToList (key: value: ''
      if [ -z "''${${key}:-}" ]; then
        export ${key}=${lib.escapeShellArg (toString value)}
      fi
    '')
    defaultEnv;
  fixed = lib.mapAttrsToList (key: value: "export ${key}=${lib.escapeShellArg (toString value)}") env;
in
  assert lib.assertMsg (lib.all validName (builtins.attrNames env ++ builtins.attrNames defaultEnv)) "harbor-meta: invalid runtime environment variable name";
    pkgs.writeShellScriptBin name ''
      if [ "$#" -eq 0 ]; then
        echo "usage: ${name} <command> [arguments...]" >&2
        exit 64
      fi
      ${lib.optionalString (pkgs.stdenv.hostPlatform.isLinux && libraryPackages != []) ''
        export LD_LIBRARY_PATH=${lib.escapeShellArg (lib.makeLibraryPath libraryPackages)}"''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
      ''}
      ${lib.optionalString (pathPackages != []) ''
        export PATH=${lib.escapeShellArg (lib.makeBinPath pathPackages)}"''${PATH:+:$PATH}"
      ''}
      ${lib.concatStringsSep "\n" defaults}
      ${lib.concatStringsSep "\n" fixed}
      exec "$@"
    ''
