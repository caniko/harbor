{
  pkgs,
  cudaPkgs,
  inputs,
  cuda,
}: let
  inherit (pkgs) lib;
  inherit (cudaPkgs) cudaPackages;
in
  pkgs.stdenv.mkDerivation {
    pname = "harbor-cad-openlb-${
      if cuda
      then "cuda"
      else "cpu"
    }";
    version = "1.9.0";
    src = inputs.openlb;
    patches = [./patches/openlb-vtk-output-precision.patch];
    nativeBuildInputs = [pkgs.gnumake] ++ lib.optionals cuda [cudaPackages.cuda_nvcc];
    buildInputs = [pkgs.zlib pkgs.tinyxml-2 pkgs.nlohmann_json] ++ lib.optionals cuda [cudaPackages.cuda_cudart];
    dontConfigure = true;
    preBuild = ''
      cat > config.mk <<'EOF'
      CXX := ${
        if cuda
        then "nvcc"
        else "g++"
      }
      CC := gcc
      CXXFLAGS := -O2 -std=c++20 -I${pkgs.nlohmann_json}/include -I${pkgs.tinyxml-2}/include
      PARALLEL_MODE := NONE
      PLATFORMS := CPU_SISD ${lib.optionalString cuda "GPU_CUDA"}
      CUDA_ARCH := 86
      FLOATING_POINT_TYPE := double
      USE_EMBEDDED_DEPENDENCIES := OFF
      EOF
      mkdir harbor-driver
      cp ${../adapters/openlb.cpp} harbor-driver/harbor-cad-openlb.cpp
      cat > harbor-driver/Makefile <<'EOF'
      OLB_ROOT := ..
      EXAMPLE := harbor-cad-openlb
      include $(OLB_ROOT)/default.mk
      EOF
      ${lib.optionalString cuda ''
        # Stubs are link-time support only, never an admitted runtime driver.
        echo 'CUDA_LDFLAGS := -L${lib.getLib cudaPackages.cuda_cudart}/lib -L${lib.getLib cudaPackages.cuda_cudart}/lib/stubs' >> config.mk
      ''}
    '';
    buildPhase = ''
      runHook preBuild
      make -C harbor-driver -j2
      runHook postBuild
    '';
    installPhase = ''
      mkdir -p $out/bin $out/share/licenses/openlb
      cp harbor-driver/harbor-cad-openlb $out/bin/
      cp COPYING $out/share/licenses/openlb/ 2>/dev/null || cp LICENSE* $out/share/licenses/openlb/
    '';
    meta.license = lib.licenses.gpl2Plus;
    passthru = {
      precision = "float64";
      sourceRevision = inputs.openlb.rev;
      cudaArchitecture =
        if cuda
        then "sm_86"
        else null;
      qualification = "unqualified";
    };
  }
