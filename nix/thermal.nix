{
  pkgs,
  fem,
}: let
  bridge = pkgs.replaceVars ../adapters/thermal_history.py {
    fem_bridge = "${fem.bridge}";
    calculix = "${fem.calculix}/bin/ccx";
    ccx_version = fem.calculix.version;
    gmsh_version = fem.gmsh.version;
  };
  adapter = pkgs.writeShellScriptBin "harbor-cad-thermal" ''
    exec ${pkgs.python313}/bin/python3 ${bridge} "$@"
  '';
in {
  thermal-cpu = adapter;
  runtime-thermal-cpu = pkgs.writeText "harbor-cad-thermal-runtime.json" (builtins.toJSON {
    schema_version = 1;
    bwrap = "${pkgs.bubblewrap}/bin/bwrap";
    thermal = "${adapter}/bin/harbor-cad-thermal";
    backend = "cpu";
    qualification = "unqualified";
  });
}
