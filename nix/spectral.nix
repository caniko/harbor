{pkgs}: let
  # Dedicated immutable Python 3.13 ABI; do not mix FreeCAD, ParaView or MCP.
  python = pkgs.python313;
  drjit = pkgs.python313Packages.buildPythonPackage {
    pname = "drjit";
    version = "1.5.0";
    format = "wheel";
    src = pkgs.fetchurl {
      url = "https://files.pythonhosted.org/packages/0b/c2/01664e2107b500a5468f1f214baabe25c872ef095f6cd591ec2ad8892ccc/drjit-1.5.0-cp313-cp313-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl";
      hash = "sha256-M6SxRsxWoC6g3ZxDA0J3xZrjxd00kGeMLLyO9poejJM=";
    };
    nativeBuildInputs = [pkgs.autoPatchelfHook];
    buildInputs = [pkgs.stdenv.cc.cc.lib];
    pythonImportsCheck = ["drjit"];
    meta = {
      description = "Exact Dr.Jit ABI for the isolated synthetic spectral CPU reference";
      homepage = "https://github.com/mitsuba-renderer/drjit";
      license = pkgs.lib.licenses.bsd3;
      platforms = ["x86_64-linux"];
    };
  };
  mitsuba = pkgs.python313Packages.buildPythonPackage {
    pname = "mitsuba";
    version = "3.9.1";
    format = "wheel";
    src = pkgs.fetchurl {
      url = "https://files.pythonhosted.org/packages/1a/83/630c3022e3207e4e918c2b4982689c3a98f92fa85d3d5056be0755ac85f2/mitsuba-3.9.1-cp313-cp313-manylinux_2_28_x86_64.whl";
      hash = "sha256-iVno3jNCfPTZtRWlLXQdynYk58FwlMWEnxI6llBMoSM=";
    };
    nativeBuildInputs = [pkgs.autoPatchelfHook];
    buildInputs = [pkgs.stdenv.cc.cc.lib pkgs.zlib];
    dependencies = [drjit];
    preFixup = ''
      addAutoPatchelfSearchPath ${drjit}/${python.sitePackages}/drjit
    '';
    pythonImportsCheck = ["mitsuba"];
    postFixup = ''
      PYTHONPATH="$out/${python.sitePackages}:${drjit}/${python.sitePackages}" ${python.interpreter} -c 'import importlib.metadata as md, mitsuba as mi; assert md.version("mitsuba") == "3.9.1"; assert md.version("drjit") == "1.5.0"; mi.set_variant("scalar_spectral"); assert mi.variant() == "scalar_spectral"'
    '';
    meta = {
      description = "Pinned native Mitsuba spectral plugins for CPU reference qualification";
      homepage = "https://github.com/mitsuba-renderer/mitsuba3";
      license = pkgs.lib.licenses.bsd3;
      platforms = ["x86_64-linux"];
    };
  };
  environment = python.withPackages (_: [mitsuba]);
  femBridge = pkgs.writeText "fem_reference.py" (builtins.readFile ../adapters/fem_reference.py);
  bridge = pkgs.replaceVars ../adapters/spectral_reference.py {
    fem_bridge = femBridge;
  };
  adapter = pkgs.writeShellScriptBin "harbor-cad-spectral" ''
    exec ${environment}/bin/python3 -B ${bridge} "$@"
  '';
  closure = pkgs.closureInfo {rootPaths = [adapter];};
in {
  spectral-environment-cpu = environment;
  spectral-mitsuba = mitsuba;
  spectral-drjit = drjit;
  spectral-reference-cpu = adapter;
  runtime-spectral-reference-cpu = pkgs.writeText "harbor-cad-spectral-reference-runtime.json" (builtins.toJSON {
    schema_version = 1;
    bwrap = "${pkgs.bubblewrap}/bin/bwrap";
    spectral = "${adapter}/bin/harbor-cad-spectral";
    spectral_closure = "${closure}/store-paths";
    backend = "cpu";
    precision = "Float32";
    qualification = "unqualified";
  });
}
