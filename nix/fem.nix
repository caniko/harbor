{pkgs}: let
  # Independent CPU reference ABI; no FreeCAD/MCP/ParaView interpreter mixing.
  gmsh =
    (pkgs.gmsh.override {
      enablePython = true;
      python3Packages = pkgs.python313Packages;
    }).overrideAttrs (old: {
      cmakeFlags = old.cmakeFlags ++ ["-DENABLE_FLTK=OFF" "-DENABLE_OPENGL=OFF"];
    });
  calculix = pkgs.calculix-ccx;
  bridge = pkgs.replaceVars ../adapters/fem_reference.py {
    gmsh_module = "${gmsh}/${pkgs.python313.sitePackages}";
    gmsh_version = gmsh.version;
    calculix = "${calculix}/bin/ccx";
    ccx_version = calculix.version;
  };
  adapter = pkgs.writeShellScriptBin "harbor-cad-fem" ''
    exec ${pkgs.python313}/bin/python3 ${bridge} "$@"
  '';
in {
  fem-cpu = adapter;
  inherit gmsh calculix;
  runtime-fem-cpu = pkgs.writeText "harbor-cad-fem-reference-runtime.json" (builtins.toJSON {
    schema_version = 1;
    bwrap = "${pkgs.bubblewrap}/bin/bwrap";
    fem = "${adapter}/bin/harbor-cad-fem";
    backend = "cpu";
    qualification = "unqualified";
  });
}
