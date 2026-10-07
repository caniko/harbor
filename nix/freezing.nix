{
  pkgs,
  inputs,
  cudaPkgs,
}: let
  base = import ./openlb.nix {
    inherit pkgs inputs cudaPkgs;
    backend = "cpu";
  };
  native = base.overrideAttrs (old: {
    pname = "harbor-cad-openlb-freezing-cpu";
    preBuild =
      old.preBuild
      + ''
        cp ${../adapters/openlb_freezing.cpp} harbor-driver/harbor-cad-openlb.cpp
      '';
    installPhase = ''
      mkdir -p $out/bin $out/share/licenses/openlb
      cp harbor-driver/harbor-cad-openlb $out/bin/harbor-cad-openlb-freezing
      cp COPYING $out/share/licenses/openlb/ 2>/dev/null || cp LICENSE* $out/share/licenses/openlb/
    '';
    passthru =
      old.passthru
      // {
        formulation = "conduction_stefan_solidification_2d";
        dimensionality = 2;
        driverSha256 = builtins.hashFile "sha256" ../adapters/openlb_freezing.cpp;
      };
  });
  femBridge = pkgs.writeText "fem_reference.py" (builtins.readFile ../adapters/fem_reference.py);
  bridge = pkgs.replaceVars ../adapters/freezing_reference.py {
    fem_bridge = femBridge;
    native = "${native}/bin/harbor-cad-openlb-freezing";
  };
  adapter = pkgs.writeShellScriptBin "harbor-cad-freezing" ''
    exec ${pkgs.python313}/bin/python3 -B ${bridge} "$@"
  '';
  closure = pkgs.closureInfo {rootPaths = [adapter];};
in {
  freezing-native-cpu = native;
  freezing-reference-cpu = adapter;
  runtime-freezing-reference-cpu = pkgs.writeText "harbor-cad-freezing-reference-runtime.json" (builtins.toJSON {
    schema_version = 1;
    bwrap = "${pkgs.bubblewrap}/bin/bwrap";
    freezing = "${adapter}/bin/harbor-cad-freezing";
    freezing_closure = "${closure}/store-paths";
    backend = "cpu";
    qualification = "unqualified";
  });
}
