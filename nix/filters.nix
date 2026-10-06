{pkgs}: let
  inherit (pkgs) lib;
  rocm = pkgs.rocmPackages;
  architecture = "gfx1100";
  compilerFlags = [
    "-DCMAKE_INSTALL_BINDIR=bin"
    "-DCMAKE_INSTALL_LIBDIR=lib"
    "-DCMAKE_INSTALL_INCLUDEDIR=include"
    "-DCMAKE_HIP_COMPILER=${rocm.llvm.clang}/bin/clang++"
    "-DCMAKE_HIP_COMPILER_ROCM_ROOT=${rocm.clr}"
    "-DCMAKE_HIP_ARCHITECTURES=${architecture}"
    "-DCMAKE_CXX_STANDARD=17"
  ];
  # Use the Kokkos version exercised by this exact bundled Viskores source's
  # upstream HIP CI, rather than Nixpkgs' unrelated default CPU Kokkos build.
  kokkos = rocm.rocmClangStdenv.mkDerivation {
    pname = "harbor-cad-kokkos-hip";
    version = "4.3.01";
    src = pkgs.fetchurl {
      url = "https://codeload.github.com/kokkos/kokkos/tar.gz/6ecdf605e0f7639adec599d25cf0e206d7b8f9f5";
      name = "kokkos-6ecdf605e0f7639adec599d25cf0e206d7b8f9f5.tar.gz";
      hash = "sha256-X1ApGi78zM48aG3IkPft1FWK+2dJ3kKl9lrq+sWleWc=";
    };
    nativeBuildInputs = [pkgs.cmake pkgs.ninja];
    buildInputs = [rocm.clr rocm.rocm-runtime];
    cmakeFlags =
      compilerFlags
      ++ [
        "-DCMAKE_POSITION_INDEPENDENT_CODE=ON"
        "-DBUILD_SHARED_LIBS=ON"
        "-DKokkos_ENABLE_HIP=ON"
        "-DKokkos_ENABLE_SERIAL=ON"
        "-DKokkos_ENABLE_OPENMP=OFF"
        "-DKokkos_ENABLE_TESTS=OFF"
        "-DKokkos_ENABLE_EXAMPLES=OFF"
        "-DKokkos_ENABLE_COMPILE_AS_CMAKE_LANGUAGE=ON"
        "-DKokkos_ENABLE_HIP_RELOCATABLE_DEVICE_CODE=OFF"
        "-DKokkos_ARCH_AMD_GFX1100=ON"
      ];
    enableParallelBuilding = true;
    passthru = {
      sourceRevision = "6ecdf605e0f7639adec599d25cf0e206d7b8f9f5";
      inherit architecture;
      qualification = "unqualified";
    };
    meta.license = lib.licenses.bsd3;
  };
  vtk = rocm.rocmClangStdenv.mkDerivation {
    pname = "harbor-cad-vtk-viskores-hip";
    version = "paraview-${pkgs.paraview.version}";
    # Full ParaView source archive contains its exact bundled VTK/Viskores.
    inherit (pkgs.paraview) src;
    postUnpack = ''sourceRoot="$sourceRoot/VTK"'';
    nativeBuildInputs = [pkgs.cmake pkgs.ninja];
    buildInputs = [kokkos rocm.clr rocm.rocm-runtime pkgs.zlib];
    postPatch = ''
      sha256sum --check <<'EOF'
      9ecd8894829fb7ae030a839b1f7abc84215ce5f87a8a6edc735a25c7030cabaa  ThirdParty/viskores/vtkviskores/CMakeLists.txt
      9887d5e91a1f04ac54dea459eeffccc9bba1a632be448821d1234e38ee5f37a5  Accelerators/Vtkm/Filters/vtkmGradient.cxx
      EOF
      # Preserve default coordinates/intermediates as Float64 in this separate
      # compute package. The independent EGL renderer retains its own ABI.
      substituteInPlace ThirdParty/viskores/vtkviskores/CMakeLists.txt \
        --replace-fail 'set(Viskores_USE_DOUBLE_PRECISION OFF)' 'set(Viskores_USE_DOUBLE_PRECISION ON)'
    '';
    cmakeFlags =
      compilerFlags
      ++ [
        "-DCMAKE_INSTALL_DOCDIR=share/doc/vtk"
        "-DKokkos_DIR=${kokkos}/lib/cmake/Kokkos"
        "-DKokkos_DEVICES=HIP;SERIAL"
        "-DVTK_USE_KOKKOS=ON"
        "-DVTK_USE_CUDA=OFF"
        "-DVTK_USE_64BIT_IDS=ON"
        "-DVTK_BUILD_TESTING=OFF"
        "-DBUILD_TESTING=OFF"
        "-DVTK_BUILD_EXAMPLES=OFF"
        "-DVTK_WRAP_PYTHON=OFF"
        "-DVTK_USE_MPI=OFF"
        "-DVTK_GROUP_ENABLE_StandAlone=DONT_WANT"
        "-DVTK_GROUP_ENABLE_Rendering=NO"
        "-DVTK_GROUP_ENABLE_Imaging=DONT_WANT"
        "-DVTK_GROUP_ENABLE_Views=NO"
        "-DVTK_GROUP_ENABLE_Web=NO"
        "-DVTK_MODULE_ENABLE_VTK_AcceleratorsVTKmFilters=YES"
        "-DVTK_MODULE_ENABLE_VTK_IOXML=YES"
        "-DVTK_USE_EXTERNAL=OFF"
      ];
    enableParallelBuilding = true;
    passthru = {
      inherit kokkos architecture;
      vtkRevision = "7c0494a68bff379d32d6b1fbaa3d10d27a73af54";
      viskoresRevision = "521f3b72aabe0bf37e9972975700df27adbbae71";
      qualification = "unqualified";
      precision = "float64";
    };
    meta.license = lib.licenses.bsd3;
  };
in {
  kokkos-hip = kokkos;
  vtk-hip = vtk;
}
