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
    pname = "harbor-cad-openlb-wetting-cpu";
    patches = old.patches ++ [./patches/openlb-wetting-initial-center.patch];
    preBuild =
      old.preBuild
      + ''
        cp ${../adapters/openlb_wetting.cpp} harbor-driver/harbor-cad-openlb.cpp
      '';
    installPhase = ''
      mkdir -p $out/bin $out/share/licenses/openlb
      cp harbor-driver/harbor-cad-openlb $out/bin/harbor-cad-openlb-wetting
      cp COPYING $out/share/licenses/openlb/ 2>/dev/null || cp LICENSE* $out/share/licenses/openlb/
    '';
    passthru =
      old.passthru
      // {
        formulation = "well_balanced_contact_angle_2d";
        dimensionality = 2;
      };
  });
  femBridge = pkgs.writeText "fem_reference.py" (builtins.readFile ../adapters/fem_reference.py);
  bridge = pkgs.replaceVars ../adapters/wetting_reference.py {
    fem_bridge = femBridge;
    native = "${native}/bin/harbor-cad-openlb-wetting";
  };
  adapter = pkgs.writeShellScriptBin "harbor-cad-wetting" ''
    exec ${pkgs.python313}/bin/python3 -B ${bridge} "$@"
  '';
  closure = pkgs.closureInfo {rootPaths = [adapter];};
in {
  wetting-reference-cpu = adapter;
  runtime-wetting-reference-cpu = pkgs.writeText "harbor-cad-wetting-reference-runtime.json" (builtins.toJSON {
    schema_version = 1;
    bwrap = "${pkgs.bubblewrap}/bin/bwrap";
    wetting = "${adapter}/bin/harbor-cad-wetting";
    wetting_closure = "${closure}/store-paths";
    backend = "cpu";
    qualification = "unqualified";
  });
}
