{
  pkgs,
  inputs,
  cudaPkgs,
}: let
  base = import ./openlb.nix {
    inherit pkgs inputs cudaPkgs;
    backend = "cpu";
  };
  driver = ../adapters/openlb_retained_cooling.cpp;
  driverSha256 = builtins.hashFile "sha256" driver;
  native = base.overrideAttrs (old: {
    pname = "harbor-cad-openlb-retained-cooling-cpu";
    preBuild =
      old.preBuild
      + ''
        cp ${driver} harbor-driver/harbor-cad-openlb.cpp
      '';
    installPhase = ''
      mkdir -p $out/bin $out/share/licenses/openlb $out/share/harbor-cad
      cp harbor-driver/harbor-cad-openlb $out/bin/harbor-cad-openlb-retained-cooling
      cp ${driver} $out/share/harbor-cad/openlb-retained-cooling.cpp
      cp COPYING $out/share/licenses/openlb/ 2>/dev/null || cp LICENSE* $out/share/licenses/openlb/
    '';
    passthru =
      old.passthru
      // {
        inherit driverSha256;
        formulation = "stationary_equal_property_retained_phase_conduction";
        dimensionality = 2;
      };
  });
  bridge = pkgs.replaceVars ../adapters/retained_cooling_bridge.py {
    native = "${native}/bin/harbor-cad-openlb-retained-cooling";
    driver_sha256 = driverSha256;
  };
  bridges = pkgs.runCommand "harbor-cad-retained-cooling-bridges" {} ''
    mkdir -p $out
    cp ${bridge} $out/retained_cooling_bridge.py
    cp ${../adapters/retained_cooling_reference.py} $out/retained_cooling_reference.py
    cp ${../adapters/fem_reference.py} $out/fem_reference.py
  '';
  adapter = pkgs.writeShellScriptBin "harbor-cad-retained-cooling" ''
    exec ${pkgs.python313}/bin/python3 -B ${bridges}/retained_cooling_bridge.py "$@"
  '';
  closure = pkgs.closureInfo {rootPaths = [adapter];};
in {
  retained-cooling-native-cpu = native;
  retained-cooling-reference-cpu = adapter;
  runtime-retained-cooling-reference-cpu = pkgs.writeText "harbor-cad-retained-cooling-reference-runtime.json" (builtins.toJSON {
    schema_version = 1;
    bwrap = "${pkgs.bubblewrap}/bin/bwrap";
    retained_cooling = "${adapter}/bin/harbor-cad-retained-cooling";
    retained_cooling_closure = "${closure}/store-paths";
    backend = "cpu";
    qualification = "unqualified";
  });
}
