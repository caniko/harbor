{
  pkgs,
  fem,
}: let
  bridge = pkgs.replaceVars ../adapters/contact_reference.py {
    fem_bridge = "${fem.bridge}";
    calculix = "${fem.calculix}/bin/ccx";
    ccx_version = fem.calculix.version;
    gmsh_version = fem.gmsh.version;
  };
  adapter = pkgs.writeShellScriptBin "harbor-cad-contact" ''
    exec ${pkgs.python313}/bin/python3 -B ${bridge} "$@"
  '';
  closure = pkgs.closureInfo {rootPaths = [adapter];};
in {
  contact-reference-cpu = adapter;
  runtime-contact-reference-cpu = pkgs.writeText "harbor-cad-contact-reference-runtime.json" (builtins.toJSON {
    schema_version = 1;
    bwrap = "${pkgs.bubblewrap}/bin/bwrap";
    contact = "${adapter}/bin/harbor-cad-contact";
    contact_closure = "${closure}/store-paths";
    backend = "cpu";
    qualification = "unqualified";
  });
}
