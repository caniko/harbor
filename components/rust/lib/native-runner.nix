{
  pkgs,
  runtime ? null,
  nativeRunner ? [],
}: let
  inherit (pkgs) lib;
  target = pkgs.stdenv.hostPlatform.rust.rustcTarget;
  variable = "CARGO_TARGET_${lib.toUpper (builtins.replaceStrings ["-"] ["_"] target)}_RUNNER";
  arguments = lib.optional (runtime != null) (lib.getExe runtime) ++ nativeRunner;
  launcher = pkgs.writeShellScriptBin "harbor-native-runner" ''
    exec ${lib.escapeShellArgs arguments} "$@"
  '';
in
  assert lib.assertMsg (builtins.isList nativeRunner && lib.all (arg: builtins.isString arg || builtins.isPath arg) nativeRunner) "harbor-rs: nativeRunner must be an argument list";
    if arguments == []
    then {
      env = {};
      packages = [];
    }
    else {
      env.${variable} = lib.getExe launcher;
      packages = [launcher];
    }
