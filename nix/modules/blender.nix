{self}: {
  config,
  lib,
  pkgs,
  options,
  ...
}: let
  cfg = config.programs.harbor.tools.blender;
  mcpCfg = cfg.mcp;

  blenderMcpPackage = mcpCfg.package;
  blenderMcpExtension = mcpCfg.extensionPackage;
  blenderMcpSkill = mcpCfg.skillPackage;

  # Shared session environment for the desktop and MCP launchers. Both
  # processes must share BLENDER_MCP_RUNTIME_DIR (instance registry) and
  # TMPDIR (viewport screenshot transport). Resolved here, never as literal
  # shell expressions in OpenCode JSON.
  mcpEnvHook = pkgs.writeText "blender-mcp-env.sh" ''
    # Fail closed on unsafe private directories: refuse symlinks, wrong
    # owners, or group/world-accessible modes instead of sharing the
    # instance registry or screenshot transport with another user.
    _blender_mcp_private_dir() {
      dir=$1
      case $dir in
        /*) ;;
        *) echo "blender-mcp: refusing non-absolute directory: $dir" >&2; return 1;;
      esac
      if [ -L "$dir" ]; then
        echo "blender-mcp: refusing symlinked directory: $dir" >&2
        return 1
      fi
      if [ -e "$dir" ]; then
        # Validate existing directories; never repair an unrelated path in
        # place, only create the intended private directory below.
        if [ ! -d "$dir" ]; then
          echo "blender-mcp: refusing non-directory: $dir" >&2
          return 1
        fi
        if [ "$(stat -c %u "$dir")" != "$(id -u)" ]; then
          echo "blender-mcp: refusing directory owned by another user: $dir" >&2
          return 1
        fi
        case $(stat -c %a "$dir") in
          700) ;;
          *) echo "blender-mcp: refusing directory with unsafe mode: $dir" >&2; return 1;;
        esac
      else
        mkdir -p -m 0700 "$dir"
      fi
    }
    if [ -z "''${BLENDER_MCP_RUNTIME_DIR:-}" ]; then
      if [ -n "''${XDG_RUNTIME_DIR:-}" ]; then
        BLENDER_MCP_RUNTIME_DIR="$XDG_RUNTIME_DIR/blender-mcp"
      else
        BLENDER_MCP_RUNTIME_DIR="$HOME/.local/state/canix/blender/runtime"
      fi
      export BLENDER_MCP_RUNTIME_DIR
    fi
    _blender_mcp_private_dir "$BLENDER_MCP_RUNTIME_DIR" || exit 1
    if [ -z "''${BLENDER_MCP_TMPDIR:-}" ]; then
      BLENDER_MCP_TMPDIR="$BLENDER_MCP_RUNTIME_DIR/tmp"
      export BLENDER_MCP_TMPDIR
    fi
    _blender_mcp_private_dir "$BLENDER_MCP_TMPDIR" || exit 1
    export TMPDIR="$BLENDER_MCP_TMPDIR"
    export BLENDER_MCP_WORKSPACE=${lib.escapeShellArg mcpCfg.workspace}
    mkdir -p "$BLENDER_MCP_WORKSPACE"
    export BLENDER_MCP_CACHE_DIR="''${XDG_CACHE_HOME:-$HOME/.cache}/blender-mcp"
    mkdir -p "$BLENDER_MCP_CACHE_DIR"
    # Fail-closed telemetry off switch, enforced by the packaged patch even
    # if upstream later adds the currently missing telemetry config module.
    export DISABLE_TELEMETRY=true
  '';

  blenderMcpServerLauncher = pkgs.writeShellApplication {
    name = "blender-mcp-launcher";
    runtimeInputs = [blenderMcpPackage pkgs.coreutils];
    text = ''
      # shellcheck disable=SC1091
      source ${mcpEnvHook}
      exec blender-mcp "$@"
    '';
  };

  blenderMcpDesktop = pkgs.writeShellApplication {
    name = "blender-mcp-desktop";
    runtimeInputs = [pkgs.coreutils];
    text = ''
      # shellcheck disable=SC1091
      source ${mcpEnvHook}
      # Dedicated session-local extension repository pointing at the
      # immutable packaged tree: Blender discovers the payload through the
      # repository, so nothing is installed, copied, or reinstalled at
      # launch. The normal user extension root is deliberately left alone
      # so unrelated extensions stay available; the bootstrap validates
      # this directory before use.
      export CANIX_BLENDER_MCP_REPO_DIR=${lib.escapeShellArg (
        if mcpCfg.autoConnect
        then "${config.xdg.dataHome}/blender-mcp-extension"
        else "${blenderMcpExtension}/share/blender-mcp-extension"
      )}
      export CANIX_BLENDER_MCP_PERSISTENT=${
        if mcpCfg.autoConnect
        then "1"
        else "0"
      }
      export CANIX_BLENDER_MCP_VERSION=${lib.escapeShellArg blenderMcpExtension.version}
      exec ${cfg.package}/bin/blender --python ${./blender-mcp-startup.py} "$@"
    '';
  };

  # One normal application entry and executable for everyday Blender work.
  # Keep the upstream desktop metadata, including MIME types and actions.
  everydayBlender = pkgs.symlinkJoin {
    name = "blender-mcp-default";
    paths = [cfg.package];
    postBuild = ''
      rm "$out/bin/blender"
      ln -s ${blenderMcpDesktop}/bin/blender-mcp-desktop "$out/bin/blender"
      desktop="$out/share/applications/blender.desktop"
      cp --remove-destination ${cfg.package}/share/applications/blender.desktop "$desktop"
      substituteInPlace "$desktop" --replace-fail 'Exec=blender' 'Exec=${blenderMcpDesktop}/bin/blender-mcp-desktop'
    '';
  };

  # Native configuration only: Jev/OpenCode own client launch and routing.
  # The wildcard covers new, mixed read/write and provider tools; allow only
  # the inspected non-destructive tools below. Project policy can be stricter.
  blenderAllowTools = [
    "blender_list_blender_instances"
    "blender_claim_blender_instance"
    "blender_get_active_blender_instance"
    "blender_release_blender_instance"
    "blender_get_scene_info"
    "blender_get_object_info"
    "blender_get_node_editor_context"
    "blender_list_node_trees"
    "blender_list_geometry_node_trees"
    "blender_get_node_tree_index"
    "blender_get_geometry_node_tree_index"
    "blender_query_node_graph"
    "blender_get_node_type_schema"
    "blender_get_geometry_node_type_schema"
    "blender_search_geometry_node_types"
    "blender_validate_node_tree_patch"
    "blender_validate_geometry_node_patch"
    "blender_get_viewport_screenshot"
    "blender_inspect_node_layout"
    "blender_audit_node_layout"
    "blender_get_runtime_automation_context"
    "blender_audit_external_dependencies"
    "blender_inspect_evaluated_mesh"
    "blender_get_simulation_status"
  ];
in {
  config = lib.mkMerge [
    (lib.mkIf (config.programs.harbor.enable && cfg.enable) {
      home.packages = [
        (
          if mcpCfg.enable && mcpCfg.autoConnect
          then lib.hiPrio everydayBlender
          else cfg.package
        )
      ];
    })

    (lib.mkIf (config.programs.harbor.enable && mcpCfg.enable && pkgs.stdenv.hostPlatform.isLinux) {
      assertions = [
        {
          assertion = cfg.enable;
          message = "harbor: Blender MCP requires tools.blender.enable.";
        }
        {
          assertion = lib.hasPrefix "/" mcpCfg.workspace;
          message = "harbor: Blender MCP workspace must be an absolute path.";
        }
        {
          assertion = blenderMcpPackage.version == blenderMcpExtension.version;
          message = "harbor: Blender MCP server and extension must use the same version.";
        }
      ];

      home.packages = [
        blenderMcpServerLauncher
        blenderMcpDesktop
        blenderMcpPackage
      ];

      # Prepare only the Canix-owned workspace directory. Extension
      # registration happens per-process against the immutable packaged
      # tree; unrelated user extensions and preferences are never touched.
      home.activation.blenderMcpExtension = lib.hm.dag.entryAfter ["writeBoundary"] ''
        run mkdir -p ${lib.escapeShellArg mcpCfg.workspace}
      '';

      # Stable path may safely be saved in Blender preferences across upgrades.
      xdg.dataFile."blender-mcp-extension" = {
        source = "${blenderMcpExtension}/share/blender-mcp-extension";
      };

      xdg.dataFile."applications/blender-mcp.desktop" = lib.mkIf (!mcpCfg.autoConnect) {
        text = ''
          [Desktop Entry]
          Type=Application
          Name=Blender (MCP)
          Comment=Blender with the pinned Blender MCP extension enabled session-local
          Exec=${blenderMcpDesktop}/bin/blender-mcp-desktop %F
          Icon=blender
          Categories=Graphics;3DGraphics;
          Terminal=false
        '';
      };

      # User policy controls connection; OpenCode's /mcps can connect on demand.
      # Neither Blender nor this module owns provider routing.
      programs.harbor.mcp.servers.blender = {
        command = "${blenderMcpServerLauncher}/bin/blender-mcp-launcher";
        enabled = mcpCfg.autoConnect;
        timeoutMs = 210000;
        # These clients have no equivalent per-server request timeout. Use
        # their native default explicitly; OpenCode, OMP and Codex retain 210s.
        overrides = lib.genAttrs ["claude" "copilot" "kiro" "zed"] (_: {timeoutMs = null;});
      };
    })
    (lib.optionalAttrs (lib.hasAttrByPath ["programs" "opencode" "settings"] options) {
      programs.opencode.settings = lib.mkIf (config.programs.harbor.enable && mcpCfg.enable && config.programs.opencode.enable) {
        permission =
          lib.mapAttrs (_: value: lib.mkDefault value)
          ({"blender_*" = "ask";} // lib.genAttrs blenderAllowTools (_: "allow"));
      };
    })
    (lib.optionalAttrs (lib.hasAttrByPath ["programs" "opencode" "skills"] options) {
      programs.opencode.skills = lib.mkIf (config.programs.harbor.enable && mcpCfg.enable && config.programs.opencode.enable) {
        blender-mcp = "${blenderMcpSkill}/share/skills/blender-mcp";
      };
    })
  ];
}
