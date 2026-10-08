{pkgs}: let
  # Public CPU DISORT implementation and data only; no private MYSTIC,
  # GPU backend, GUI or dynamically fetched atmospheric data.
  sourceSha256 = "64930cc40b6e4a37aa220520974d330fc1563796f466a649b2238131f2d69840";
  native = pkgs.stdenv.mkDerivation {
    pname = "harbor-cad-libradtran";
    version = "2.0.6";
    src = pkgs.fetchurl {
      url = "https://www.libradtran.org/download/libRadtran-2.0.6.tar.gz";
      sha256 = sourceSha256;
    };
    strictDeps = true;
    nativeBuildInputs = [
      pkgs.gfortran
      pkgs.flex
      pkgs.python313
      pkgs.pkg-config
      # configure executes nc-config/nf-config; strictDeps keeps target
      # libraries off PATH, so these helpers also belong to the native inputs.
      (pkgs.lib.getBin pkgs.netcdf)
      (pkgs.lib.getBin pkgs.netcdffortran)
    ];
    # The pinned configure.in unconditionally links both GSL and gslcblas.
    buildInputs = [pkgs.netcdf pkgs.netcdffortran pkgs.gsl];
    # The distribution's fixed-form legacy Fortran routines use the original
    # F77 calling convention. This compiler mode does not reduce arithmetic
    # precision or enable unsafe floating-point optimization.
    env.FFLAGS = "-O2 -std=legacy";
    enableParallelBuilding = true;
    buildPhase = ''
      runHook preBuild
      make -j"$NIX_BUILD_CORES" uvspec
      runHook postBuild
    '';
    doCheck = true;
    checkPhase = ''
      runHook preCheck
      printf '280 1\n320 2\n400 3\n' > solar-reference.dat
      cat > transparent.inp <<EOF
      data_files_path $PWD/data
      atmosphere_file $PWD/data/atmmod/afglms.dat
      source solar $PWD/solar-reference.dat per_nm
      wavelength 280 400
      mol_abs_param crs
      sza 30
      albedo 0
      rte_solver disort
      number_of_streams 8
      zout 0
      no_absorption
      no_scattering
      output_user lambda edir edn eup
      quiet
      EOF
      bin/uvspec < transparent.inp > transparent-original.txt
      ${pkgs.python313.interpreter} - <<'PY'
      import math
      from pathlib import Path
      rows = [list(map(float, row.split())) for row in Path("transparent-original.txt").read_text().splitlines()]
      assert len(rows) == 3
      for row, wavelength, flux in zip(rows, (280, 320, 400), (1, 2, 3), strict=True):
          assert len(row) == 4 and row[0] == wavelength
          assert math.isclose(row[1], flux * math.cos(math.pi / 6), rel_tol=1e-6)
          assert row[2:] == [0, 0]
      PY
      runHook postCheck
    '';
    installPhase = ''
      runHook preInstall
      install -Dm755 bin/uvspec $out/bin/uvspec
      mkdir -p $out/share/libRadtran $out/share/harbor-cad-libradtran
      cp -r data $out/share/libRadtran/data
      cp COPYING INSTALL $out/share/harbor-cad-libradtran/
      cp transparent.inp transparent-original.txt $out/share/harbor-cad-libradtran/
      printf '%s\n' '${sourceSha256}' > $out/share/harbor-cad-libradtran/source.sha256
      runHook postInstall
    '';
    meta = {
      description = "Pinned public libRadtran CPU DISORT for explicit synthetic UV atmosphere references";
      homepage = "https://www.libradtran.org/";
      license = pkgs.lib.licenses.gpl2Plus;
      platforms = ["x86_64-linux"];
    };
  };
  bridge = pkgs.writeText "atmosphere_reference.py" (builtins.readFile ../adapters/atmosphere_reference.py);
  adapter = pkgs.writeShellScriptBin "harbor-cad-atmosphere" ''
    export HARBOR_CAD_LIBRADTRAN=${native}/bin/uvspec
    export HARBOR_CAD_LIBRADTRAN_DATA=${native}/share/libRadtran/data
    exec ${pkgs.python313.interpreter} -B ${bridge} "$@"
  '';
  closure = pkgs.closureInfo {rootPaths = [adapter];};
in {
  atmosphere-native-cpu = native;
  atmosphere-reference-cpu = adapter;
  runtime-atmosphere-worker = pkgs.writeText "harbor-cad-native-runtime.json" (builtins.toJSON {
    bwrap = "${pkgs.bubblewrap}/bin/bwrap";
    atmosphere = "${adapter}/bin/harbor-cad-atmosphere";
    atmosphere_closure = "${closure}/store-paths";
    cad = null;
    openlb = null;
    openlb_backend = "cpu";
    render = null;
    video = null;
  });
  runtime-atmosphere-reference-cpu = pkgs.writeText "harbor-cad-atmosphere-runtime.json" (builtins.toJSON {
    schema_version = 1;
    bwrap = "${pkgs.bubblewrap}/bin/bwrap";
    atmosphere = "${adapter}/bin/harbor-cad-atmosphere";
    atmosphere_closure = "${closure}/store-paths";
    backend = "cpu";
    solver = "disort";
    precision = "Float32";
    source_sha256 = sourceSha256;
    qualification = "unqualified";
  });
}
