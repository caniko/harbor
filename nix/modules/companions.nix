{
  self,
  kind,
}: {
  config,
  lib,
  pkgs,
  ...
}: let
  inherit (lib) mkOption types;
  cfg = config.programs.harbor;
  system = pkgs.stdenv.hostPlatform.system;
  backendNames = map (lib.removePrefix "cad-runtime-") (lib.filter (lib.hasPrefix "cad-runtime-") (builtins.attrNames self.packages.x86_64-linux));
in {
  options.programs.harbor.tools = {
    blender.mcp = {
      enable = mkOption {
        type = types.bool;
        default = cfg.tools.blender.enable && pkgs.stdenv.hostPlatform.isLinux;
      };
      autoConnect = mkOption {
        type = types.bool;
        default = true;
        description = "Connect enabled clients and use the MCP-aware everyday Blender launcher in Home Manager.";
      };
      package = mkOption {
        type = types.package;
        default = self.packages.${system}.blender-mcp;
      };
      extensionPackage = mkOption {
        type = types.package;
        default = self.packages.${system}.blender-mcp-extension;
      };
      skillPackage = mkOption {
        type = types.package;
        default = self.packages.${system}.blender-mcp-skill;
      };
      workspace = mkOption {
        type = types.str;
        default =
          if kind == "home"
          then "${config.xdg.stateHome}/canix/blender/workspace"
          else "";
        description = "Absolute writable JSON export/patch workspace for Home Manager. The existing canix state identity is preserved.";
      };
    };
    freecad.mcp = {
      enable = mkOption {
        type = types.bool;
        default = false;
        description = "Install and register the FreeCAD MCP server (neka-nat/freecad-mcp). Opt-in: its FreeCAD add-on serves unauthenticated RPC on 127.0.0.1.";
      };
      addon.enable = mkOption {
        type = types.bool;
        default = cfg.tools.freecad.mcp.enable;
        defaultText = lib.literalExpression "tools.freecad.mcp.enable";
        description = "Install the FreeCAD-side add-on and localhost-only RPC settings into the FreeCAD user profile on Home Manager activation.";
      };
      onlyTextFeedback = mkOption {
        type = types.bool;
        default = false;
        description = "Suppress screenshot/image feedback from the MCP server.";
      };
      package = mkOption {
        type = types.package;
        default = self.packages.${system}.freecad-mcp;
      };
      addonPackage = mkOption {
        type = types.package;
        default = self.packages.${system}.freecad-mcp-addon;
      };
    };
    openpencil = {
      desktop.enable = mkOption {
        type = types.bool;
        default = cfg.tools.openpencil.enable && system == "x86_64-linux";
      };
      desktop.package = mkOption {
        type = types.package;
        default = self.packages.${system}.open-pencil-desktop.override {cli = cfg.tools.openpencil.package;};
      };
      mcp.enable = mkOption {
        type = types.bool;
        default = cfg.tools.openpencil.desktop.enable;
      };
    };
    cad.backends = mkOption {
      type = types.listOf (types.enum backendNames);
      default =
        if cfg.tools.cad.enable && system == "x86_64-linux"
        then ["cpu"]
        else [];
      description = "Installed immutable CAD runtime manifests. CPU is the full preset; CUDA/HIP and specialized backends are explicit selections. Driver and worker-profile policy remain consumer-owned.";
    };
    cad.runtimeManifests = mkOption {
      type = types.attrsOf types.package;
      readOnly = true;
      default =
        if cfg.enable && lib.elem "cad" cfg.resolved.selected
        then lib.genAttrs cfg.tools.cad.backends (backend: self.packages.${system}."cad-runtime-${backend}")
        else {};
    };
    cad.mcp =
      {
        enable = mkOption {
          type = types.bool;
          default = cfg.tools.cad.enable;
        };
        socket = mkOption {
          type = types.nullOr types.str;
          default = null;
          description = "Existing user worker socket. Persistent worker activation still requires programs.harbor-cad.worker configuration.";
        };
        profile = mkOption {
          type = types.enum ["cad" "simulation" "results" "all"];
          default = "all";
        };
      }
      // lib.optionalAttrs (kind != "home") {
        package = mkOption {
          type = types.package;
          default = self.packages.${system}.cad-mcp;
        };
      };
  };
  config = lib.mkIf cfg.enable (lib.mkMerge [
    {
      assertions = [
        {
          assertion = !cfg.tools.blender.mcp.enable || cfg.tools.blender.enable;
          message = "harbor: Blender MCP requires tools.blender.enable";
        }
        {
          assertion = !cfg.tools.blender.mcp.enable || pkgs.stdenv.hostPlatform.isLinux;
          message = "harbor: Blender MCP launchers currently support Linux only";
        }
        {
          assertion = !(cfg.tools.freecad.mcp.enable || cfg.tools.freecad.mcp.addon.enable) || cfg.tools.freecad.enable;
          message = "harbor: FreeCAD MCP requires tools.freecad.enable";
        }
        {
          # Only the Home Manager module installs and registers the server.
          assertion = kind == "home" || !(cfg.tools.freecad.mcp.enable || cfg.tools.freecad.mcp.addon.enable);
          message = "harbor: FreeCAD MCP is supported only through Home Manager";
        }
        {
          assertion = !cfg.tools.openpencil.desktop.enable || cfg.tools.openpencil.enable;
          message = "harbor: OpenPencil desktop requires tools.openpencil.enable";
        }
        {
          assertion = !cfg.tools.openpencil.desktop.enable || system == "x86_64-linux";
          message = "harbor: OpenPencil desktop currently supports x86_64-linux only";
        }
        {
          assertion =
            !cfg.tools.openpencil.desktop.enable
            || system != "x86_64-linux"
            || (cfg.tools.openpencil.desktop.package.cli or null) == cfg.tools.openpencil.package;
          message = "harbor: OpenPencil desktop must expose passthru.cli matching tools.openpencil.package";
        }
        {
          assertion =
            !cfg.tools.openpencil.desktop.enable
            || system != "x86_64-linux"
            || (cfg.tools.openpencil.desktop.package.upstreamVersion or null) == cfg.tools.openpencil.package.version;
          message = "harbor: OpenPencil desktop's passthru.upstreamVersion must match the selected CLI/MCP version";
        }
        {
          assertion = !cfg.tools.openpencil.mcp.enable || cfg.tools.openpencil.enable;
          message = "harbor: OpenPencil MCP requires tools.openpencil.enable";
        }
        {
          assertion = !cfg.tools.cad.mcp.enable || cfg.tools.cad.enable;
          message = "harbor: CAD MCP requires tools.cad.enable";
        }
        {
          assertion = cfg.tools.cad.backends == [] || cfg.tools.cad.enable;
          message = "harbor: CAD backends require tools.cad.enable";
        }
        {
          assertion = cfg.tools.cad.mcp.socket == null || lib.hasPrefix "/" cfg.tools.cad.mcp.socket;
          message = "harbor: CAD worker socket must be absolute";
        }
      ];
    }
    (lib.mkIf (kind == "home" && cfg.tools.openpencil.mcp.enable) {
      programs.harbor.mcp.servers.open-pencil.command = "${cfg.tools.openpencil.package}/bin/openpencil-mcp";
    })
    (lib.mkIf (kind == "home" && cfg.tools.cad.mcp.enable && lib.elem "cad" cfg.resolved.selected) {
      programs.harbor.mcp.servers.harbor-cad = {
        command = "${cfg.tools.cad.mcp.package}/bin/harbor-cad-mcp";
        args = ["--profile" cfg.tools.cad.mcp.profile];
        env = lib.optionalAttrs (cfg.tools.cad.mcp.socket != null) {HARBOR_CAD_SOCKET = cfg.tools.cad.mcp.socket;};
      };
    })
  ]);
}
