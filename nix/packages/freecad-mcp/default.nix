{pkgs}: let
  version = "0.1.18";
  rev = "63acb305573194a011641ab13ccfb391fe95769f";
  src = pkgs.fetchFromGitHub {
    owner = "neka-nat";
    repo = "freecad-mcp";
    inherit rev;
    hash = "sha256-MoYnDC9O4FkBjYLYR1FL9Z7R3q3/BdgMHUum47MMWfE=";
  };

  freecadMcp = pkgs.python3Packages.buildPythonApplication {
    pname = "freecad-mcp";
    inherit version;
    pyproject = true;
    inherit src;

    build-system = [pkgs.python3Packages.hatchling];
    # Upstream requires mcp[cli]>=1.12.2 on the v1 SDK line, which nixpkgs
    # keeps as `mcp_1` once `mcp` moves to 2.x.
    dependencies = with pkgs.python3Packages; [
      (pkgs.python3Packages.mcp_1 or mcp)
      validators
    ];

    pythonImportsCheck = ["freecad_mcp"];

    meta = with pkgs.lib; {
      description = "Model Context Protocol bridge for FreeCAD";
      homepage = "https://github.com/neka-nat/freecad-mcp";
      license = licenses.mit;
      mainProgram = "freecad-mcp";
      maintainers = [];
    };
  };

  freecadMcpAddon = pkgs.stdenvNoCC.mkDerivation {
    pname = "freecad-mcp-addon";
    inherit version;
    inherit src;

    dontConfigure = true;
    dontBuild = true;

    installPhase = ''
      runHook preInstall
      mkdir -p "$out/share/freecad/Mod"
      cp -r addon/FreeCADMCP "$out/share/freecad/Mod/FreeCADMCP"
      runHook postInstall
    '';

    meta = with pkgs.lib; {
      description = "FreeCAD addon payload for freecad-mcp";
      homepage = "https://github.com/neka-nat/freecad-mcp";
      license = licenses.mit;
      maintainers = [];
    };
  };
in {
  freecad-mcp = freecadMcp;
  freecad-mcp-addon = freecadMcpAddon;
}
