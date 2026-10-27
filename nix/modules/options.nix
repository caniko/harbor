{
  self,
  kind,
}: {
  config,
  options,
  lib,
  pkgs,
  osConfig ? {},
  ...
}: let
  inherit (lib) mkOption types;
  cfg = config.programs.harbor;
  tooling = import ../tooling/selection.nix {inherit lib;};
  inherit (tooling) catalog;
  system = pkgs.stdenv.hostPlatform.system;
  providers = import ../tooling/packages.nix {inherit self pkgs config;};
  selection = tooling.resolve {
    inherit system;
    inherit (cfg) enable frameworks;
    tools = lib.mapAttrs (name: tool:
      if options.programs.harbor.tools.${name}.enable.highestPrio < 1500
      then {inherit (tool) enable;}
      else {})
    cfg.tools;
  };
  binaries = lib.unique (lib.concatMap (name: catalog.tools.${name}.provides) selection.selected);
  conflicts = lib.filter (binary: let
    owners = lib.filter (name: lib.elem binary catalog.tools.${name}.provides) selection.selected;
  in
    builtins.length (lib.unique (map (name: toString cfg.tools.${name}.package) owners)) > 1)
  binaries;
  inherited = osConfig.programs.harbor or {};
in {
  options.programs.harbor = {
    enable = lib.mkEnableOption "selected Harbor framework tooling";
    frameworks = mkOption {
      type = types.listOf (types.enum (builtins.attrNames catalog.frameworks));
      default = [];
      example = ["rust" "python" "cad"];
      description = "Full curated presets. Core tools are included whenever Harbor is enabled.";
    };
    skills = mkOption {
      type = types.listOf (types.enum (builtins.attrNames catalog.skills));
      default = [];
      example = ["dataset-report"];
      description = "Catalog skills to bundle with their required tools realized into each skill's env/bin.";
    };
    skillBundle = mkOption {
      type = types.nullOr types.package;
      readOnly = true;
      internal = true;
      description = "Bundle of the selected skills and their Skillnet.pkl manifest, for consumer adapters such as homeManagerModules.skillnet.";
    };
    inheritSystemSelection = mkOption {
      type = types.bool;
      default = false;
      description = "In Home Manager, inherit the NixOS enable/framework selection as overridable defaults. System packages remain system-wide.";
    };
    tools = lib.mapAttrs (name: tool:
      {
        enable = mkOption {
          type = types.bool;
          default = tooling.defaultEnabled {
            inherit system;
            inherit (cfg) enable frameworks;
            tool = name;
          };
          defaultText = lib.literalExpression "selected preset and supported host platform";
          description = "Install ${name}; explicit values override preset defaults.";
        };
      }
      // lib.optionalAttrs (!(name == "cad" || (kind == "home" && name == "llm"))) {
        package = mkOption {
          type = types.package;
          default = providers.${name};
          defaultText = lib.literalExpression "Harbor's ${name} provider";
          description = "Package used by installation and integrations for ${name}.";
        };
      })
    catalog.tools;
    settings = {
      rust.profile = mkOption {
        type = types.enum ["stable" "nightly"];
        default = "nightly";
      };
      tex.profile = mkOption {
        type = types.enum (builtins.attrNames self.lib.tex.profiles);
        default = "article";
      };
      android = {
        platformVersions = mkOption {
          type = types.listOf types.nonEmptyStr;
          default = [];
        };
        buildToolsVersions = mkOption {
          type = types.listOf types.nonEmptyStr;
          default = [];
        };
        ndkVersions = mkOption {
          type = types.listOf types.nonEmptyStr;
          default = [];
        };
        includeNDK = mkOption {
          type = types.bool;
          default = true;
        };
        includeEmulator = mkOption {
          type = types.bool;
          default = false;
        };
        includeSystemImages = mkOption {
          type = types.bool;
          default = false;
        };
        systemImageTypes = mkOption {
          type = types.listOf types.nonEmptyStr;
          default = ["google_apis"];
        };
        abiVersions = mkOption {
          type = types.listOf types.nonEmptyStr;
          default = ["arm64-v8a"];
        };
      };
      ntt = {
        pinsFile = mkOption {
          type = types.nullOr types.path;
          default = null;
          description = "Immutable JSON pin document accepted by Harbor NTT's existing constructor.";
        };
        rustToolchain = mkOption {
          type = types.nullOr types.package;
          default = null;
        };
      };
    };
    resolved = {
      selected = mkOption {
        type = types.listOf types.str;
        readOnly = true;
      };
      unavailable = mkOption {
        type = types.listOf types.str;
        readOnly = true;
      };
      unsupported = mkOption {
        type = types.listOf types.str;
        readOnly = true;
      };
    };
    mcp.servers = mkOption {
      type = types.attrsOf (types.submodule {
        options = {
          command = mkOption {type = types.str;};
          args = mkOption {
            type = types.listOf types.str;
            default = [];
          };
          env = mkOption {
            type = types.attrsOf types.str;
            default = {};
          };
          enabled = mkOption {
            type = types.bool;
            default = true;
          };
          timeoutMs = mkOption {
            type = types.nullOr types.ints.positive;
            default = null;
          };
          overrides = mkOption {
            type = (pkgs.formats.json {}).type;
            default = {};
          };
        };
      });
      default = {};
      description = "Resolved server declarations. Client delivery is supplied by a consumer adapter, such as homeManagerModules.fleetix.";
    };
  };
  config = lib.mkMerge [
    {
      programs.harbor.resolved = selection;
      programs.harbor.skillBundle = if cfg.skills == [] then null else import ../tooling/skills.nix {inherit self pkgs config;} cfg.skills;
      assertions = [
        {
          assertion = selection.unsupported == [];
          message = "harbor: unsupported on ${system}: ${lib.concatStringsSep ", " selection.unsupported}";
        }
        {
          assertion = conflicts == [];
          message = "harbor: conflicting executable providers: ${lib.concatStringsSep ", " conflicts}. Disable one provider or override packages to share the same derivation; keep independently pinned toolchains project-scoped.";
        }
      ];
    }
    (lib.mkIf (kind == "home" && cfg.inheritSystemSelection) {
      programs.harbor = {
        enable = lib.mkDefault (inherited.enable or false);
        frameworks = lib.mkDefault (inherited.frameworks or []);
      };
    })
  ];
}
