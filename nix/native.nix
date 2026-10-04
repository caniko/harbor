{
  pkgs,
  cudaPkgs,
  inputs,
}: let
  inherit (pkgs) lib;
  freecad = assert lib.versionAtLeast pkgs.freecad.version "1.1.4"; pkgs.freecad;
  adapter = name: executable: script:
    pkgs.writeShellScriptBin name ''
      exec ${executable} ${script} "$@"
    '';
  cad = pkgs.writeShellScriptBin "harbor-cad-import" ''
    export HARBOR_CAD_OPERATION="$1" HARBOR_CAD_PLAN="$2"
    exec ${freecad}/bin/FreeCADCmd --safe-mode ${../adapters/freecad_bridge.py}
  '';
  # Internal VTK comes from the exact ParaView source archive, avoiding an ABI mix.
  paraview = pkgs.stdenv.mkDerivation {
    pname = "harbor-cad-paraview-egl";
    inherit (pkgs.paraview) version src;
    nativeBuildInputs = [pkgs.cmake pkgs.ninja];
    buildInputs = [pkgs.python313 pkgs.mesa pkgs.libGL pkgs.libglvnd pkgs.zlib];
    cmakeFlags = [
      "-DPARAVIEW_USE_QT=OFF"
      "-DPARAVIEW_USE_MPI=OFF"
      "-DPARAVIEW_USE_PYTHON=ON"
      "-DPARAVIEW_USE_EXTERNAL_VTK=OFF"
      "-DPARAVIEW_VERSIONED_INSTALL=OFF"
      "-DPARAVIEW_ENABLE_WEB=OFF"
      "-DPARAVIEW_ENABLE_CATALYST=OFF"
      "-DPARAVIEW_BUILD_TESTING=OFF"
      "-DBUILD_TESTING=OFF"
      "-DVTK_BUILD_TESTING=OFF"
      "-DVTK_USE_X=OFF"
      "-DVTK_OPENGL_HAS_EGL=ON"
      "-DVTK_DEFAULT_RENDER_WINDOW_OFFSCREEN=ON"
      "-DVTK_SMP_IMPLEMENTATION_TYPE=Sequential"
      "-DVTK_USE_EXTERNAL=OFF"
    ];
    enableParallelBuilding = true;
    doInstallCheck = true;
    installCheckPhase = ''
      env -i HOME=$TMPDIR $out/bin/pvpython -c 'from paraview import simple; from vtkmodules.vtkRenderingOpenGL2 import vtkEGLRenderWindow; assert vtkEGLRenderWindow.__name__ == "vtkEGLRenderWindow"'
    '';
    meta.license = lib.licenses.bsd3;
  };
  visualization = adapter "harbor-cad-render" "${paraview}/bin/pvpython" (pkgs.replaceVars ../adapters/paraview_bridge.py {
    libegl = "${pkgs.libglvnd}/lib/libEGL.so.1";
  });
  media = adapter "harbor-cad-video" "${pkgs.python313}/bin/python3" (pkgs.replaceVars ../adapters/video_bridge.py {
    ffmpeg = "${pkgs.ffmpeg}/bin/ffmpeg";
    ffprobe = "${pkgs.ffmpeg}/bin/ffprobe";
  });
  mkOpenlb = cuda: import ./openlb.nix {inherit pkgs cudaPkgs inputs cuda;};
  openlb-cpu = mkOpenlb false;
  openlb-cuda = mkOpenlb true;
  runtime = backend: openlb:
    pkgs.writeText "harbor-cad-native-runtime.json" (builtins.toJSON {
      bwrap = "${pkgs.bubblewrap}/bin/bwrap";
      cad = "${cad}/bin/harbor-cad-import";
      render = "${visualization}/bin/harbor-cad-render";
      video = "${media}/bin/harbor-cad-video";
      inherit openlb;
      openlb_backend = backend;
    });
in {
  inherit cad visualization media openlb-cpu openlb-cuda;
  runtime-cpu = runtime "cpu" "${openlb-cpu}/bin/harbor-cad-openlb";
  runtime-cuda = runtime "cuda" "${openlb-cuda}/bin/harbor-cad-openlb";
}
