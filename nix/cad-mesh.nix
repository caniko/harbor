{
  pkgs,
  fem,
}: let
  bridge = pkgs.replaceVars ../adapters/cad_mesh.py {
    fem_bridge = "${fem.bridge}";
    gmsh_version = fem.gmsh.version;
  };
  adapter = pkgs.writeShellScriptBin "harbor-cad-cad-mesh" ''
    exec ${pkgs.python313}/bin/python3 ${bridge} "$@"
  '';
in {
  cad-mesh-cpu = adapter;
  runtime-cad-mesh-cpu = pkgs.writeText "harbor-cad-cad-mesh-runtime.json" (builtins.toJSON {
    schema_version = 1;
    bwrap = "${pkgs.bubblewrap}/bin/bwrap";
    cad_mesh = "${adapter}/bin/harbor-cad-cad-mesh";
    backend = "cpu";
    qualification = "unqualified";
  });
}
