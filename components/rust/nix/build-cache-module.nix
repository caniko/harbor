# NixOS host integration for harbor-rs's generic derivation build policy.
#
# The sccache module owns transport and credentials.  This module owns the
# writable sandbox mount and exposes the same pure policy object to NixOS
# consumers and Crossbow assembly modules.
{
  config,
  lib,
  pkgs,
  rsHarborBuildPkgs ? pkgs.buildPackages,
  ...
}: let
  cfg = config.programs.harborRs.buildCache;
  policy = (import ../lib/build-cache.nix {}).mkBuildCachePolicy {
    inherit pkgs;
    buildPackageSet = rsHarborBuildPkgs;
    sccachePackage = cfg.package;
    cacheRoot = cfg.cacheRoot;
    namespaceScope = cfg.namespaceScope;
    namespaceGeneration = cfg.namespaceGeneration;
    connectTimeout = cfg.connectTimeout;
    executionModel = cfg.executionModel;
  };
in {
  imports = [./sccache-module.nix];

  options.programs.harborRs.buildCache = {
    enable = lib.mkEnableOption "harbor-rs derivation compiler-cache policy";

    package = lib.mkPackageOption pkgs "sccache" {};

    executionModel = lib.mkOption {
      type = lib.types.enum ["sandbox-local"];
      default = "sandbox-local";
      description = "Compiler-cache execution model used by Nix derivations.";
    };

    cacheRoot = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = "/tmp/sccache";
      description = "Optional writable host-mounted root for local caches; null selects a remote transport such as Redis.";
    };

    namespaceScope = lib.mkOption {
      type = lib.types.str;
      default = "rs-harbor-rust";
      description = "Stable fleet scope used to derive the shared cache namespace.";
    };

    namespaceGeneration = lib.mkOption {
      type = lib.types.ints.positive;
      default = 1;
      description = "Manual cache namespace generation for wrapper or ABI changes.";
    };

    connectTimeout = lib.mkOption {
      type = lib.types.str;
      default = "2";
      description = "SCCACHE_CONNECT_TIMEOUT passed by the sandbox wrapper.";
    };

    rustPackages = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [];
      description = "Rust package attributes to wrap in the active package set.";
    };

    cmakePackages = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [];
      description = "CMake package attributes to wrap in the active package set.";
    };

    contract = lib.mkOption {
      type = lib.types.raw;
      readOnly = true;
      internal = true;
      description = "Resolved harbor-rs compiler-cache contract.";
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = cfg.executionModel == "sandbox-local";
        message = "harbor-rs build cache currently supports only sandbox-local execution";
      }
      {
        assertion = cfg.cacheRoot != "/";
        message = "harbor-rs build cache refuses / as a sandbox cache root";
      }
    ];

    programs.harborRs.sccache = {
      enable = lib.mkDefault true;
      package = lib.mkDefault cfg.package;
      cacheNamespaceScope = lib.mkDefault cfg.namespaceScope;
      cacheNamespaceGeneration = lib.mkDefault cfg.namespaceGeneration;
      # The policy wrapper scopes XDG_CACHE_HOME to sccache itself.  Do not
      # export it globally from the host module.
      setGlobalEnvironment = lib.mkDefault false;
    };

    programs.harborRs.buildCache.contract = policy.contract;

    _module.args.rsHarborBuildCache = policy;

    nix.settings = {
      extra-sandbox-paths = lib.mkIf (cfg.cacheRoot != null) (lib.mkAfter [cfg.cacheRoot]);
      "impure-env" = lib.mkIf (cfg.cacheRoot != null) (lib.mkAfter ["SCCACHE_DIR=${policy.sharedCacheDir}"]);
      experimental-features = ["configurable-impure-env"];
    };

    systemd.tmpfiles.rules = lib.mkIf (cfg.cacheRoot != null) [
      "d ${policy.sharedCacheDir} 2770 root nixbld -"
    ];

    nixpkgs.overlays = [
      (policy.mkOverlay {
        rustPackages = cfg.rustPackages;
        cmakePackages = cfg.cmakePackages;
      })
    ];
  };
}
