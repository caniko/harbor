{
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.programs.harbor.tools.freecad;
  mcpCfg = cfg.mcp;

  # Localhost-only RPC; the add-on starts it with FreeCAD.
  settingsFile = pkgs.writeText "freecad_mcp_settings.json" (builtins.toJSON {
    allowed_ips = "127.0.0.1";
    auto_start_rpc = true;
    remote_enabled = false;
  });

  launcher = pkgs.writeShellApplication {
    name = "freecad-mcp-launcher";
    runtimeInputs = [mcpCfg.package];
    text = ''
      set -euo pipefail

      args=(--host 127.0.0.1)

      ${
        lib.optionalString mcpCfg.onlyTextFeedback ''
          args=(--only-text-feedback "''${args[@]}")
        ''
      }

      exec freecad-mcp "''${args[@]}"
    '';
  };
in {
  config = lib.mkIf config.programs.harbor.enable (lib.mkMerge [
    (lib.mkIf (mcpCfg.addon.enable && cfg.enable) {
      # FreeCAD only loads add-ons from its writable user Mod directory, so
      # replace the managed FreeCADMCP copy on every activation.
      home.activation.freecadMcpAddon = lib.hm.dag.entryAfter ["writeBoundary"] ''
        freecad_app_data_dir="$(${cfg.package}/bin/FreeCADCmd -c 'import FreeCAD; print(FreeCAD.getUserAppDataDir())' | tail -n 1)"
        if [ -z "$freecad_app_data_dir" ]; then
          echo "FreeCAD MCP activation failed: could not determine FreeCAD user app data directory" >&2
          exit 1
        fi

        freecad_app_data_dir="''${freecad_app_data_dir%/}"
        freecad_mod_dir="$freecad_app_data_dir/Mod"

        run mkdir -p "$freecad_mod_dir"
        run chmod -R u+w "$freecad_mod_dir/FreeCADMCP" 2>/dev/null || true
        run rm -rf "$freecad_mod_dir/FreeCADMCP"
        run cp -r ${mcpCfg.addonPackage}/share/freecad/Mod/FreeCADMCP "$freecad_mod_dir/FreeCADMCP"
        run install -m 0644 ${settingsFile} "$freecad_app_data_dir/freecad_mcp_settings.json"
      '';
    })
    (lib.mkIf mcpCfg.enable {
      home.packages = [launcher mcpCfg.package];
      programs.harbor.mcp.servers.freecad.command = "${launcher}/bin/freecad-mcp-launcher";
    })
  ]);
}
