{
  config,
  lib,
  pkgs,
  osConfig ? null,
  ...
}: let
  sccacheDefault = import ../lib/generated/sccache-default.nix;
  sccacheService = import ../lib/sccache-service.nix {inherit pkgs;};
  inherit (lib) mkIf mkOption types;

  osSccache = lib.attrByPath ["programs" "harborRs" "sccache"] {} osConfig;
  osSccacheEnabled = osConfig != null && (osSccache.enable or false);
  osSccacheRemoteEnv = osSccache.remoteEnvVars or {};

  cfg = config.programs.harborRs.sccache.userDaemon;
  reservedUserEnvironment = [
    "RUSTC_WRAPPER"
    "SCCACHE_SERVER_UDS"
    "SCCACHE_DIR"
    "XDG_CACHE_HOME"
    "SCCACHE_START_SERVER"
    "SCCACHE_NO_DAEMON"
    "SCCACHE_BASEDIRS"
    "CARGO_INCREMENTAL"
    "SCCACHE_MULTILEVEL_CHAIN"
    "SCCACHE_REDIS_ENDPOINT"
    "SCCACHE_REDIS_KEY_PREFIX"
    "SCCACHE_REDIS_RW_MODE"
    "SCCACHE_MULTILEVEL_WRITE_ERROR_POLICY"
  ];

  serviceEnv =
    osSccacheRemoteEnv
    // cfg.environment
    // lib.optionalAttrs (cfg.basedirs != []) {
      SCCACHE_BASEDIRS = lib.concatStringsSep ":" cfg.basedirs;
    }
    // lib.optionalAttrs (cfg.multiLevelChain != null) {
      SCCACHE_MULTILEVEL_CHAIN = cfg.multiLevelChain;
    }
    // lib.optionalAttrs (cfg.redisEndpoint != null) {
      SCCACHE_REDIS_ENDPOINT = cfg.redisEndpoint;
    }
    // lib.optionalAttrs (cfg.redisKeyPrefix != null) {
      SCCACHE_REDIS_KEY_PREFIX = cfg.redisKeyPrefix;
    }
    // lib.optionalAttrs (cfg.redisRwMode != null) {
      SCCACHE_REDIS_RW_MODE = cfg.redisRwMode;
    }
    // lib.optionalAttrs (cfg.multiLevelWriteErrorPolicy != null) {
      SCCACHE_MULTILEVEL_WRITE_ERROR_POLICY = cfg.multiLevelWriteErrorPolicy;
    }
    // {
      SCCACHE_SERVER_UDS = "%t/${sccacheDefault.userSocketRel}";
      SCCACHE_DIR = "${config.xdg.cacheHome}/${sccacheDefault.userCacheRel}";
      SCCACHE_IDLE_TIMEOUT = toString cfg.idleTimeout;
      SCCACHE_START_SERVER = "1";
      SCCACHE_NO_DAEMON = "1";
      SCCACHE_LOG = "warn";
    };

  serviceEnvironment = lib.mapAttrsToList (name: value: "${name}=${value}") serviceEnv;

  daemonLauncher = pkgs.writeShellScript "sccache-user-daemon-launcher" ''
    set -eu

    ${lib.optionalString (cfg.basedirsFile != null) ''
      runtime_basedirs=""
      if [ -r ${lib.escapeShellArg cfg.basedirsFile} ]; then
        while IFS= read -r basedir || [ -n "$basedir" ]; do
          [ -n "$basedir" ] || continue
          case "$basedir" in
            /*) ;;
            *)
              echo "harbor-rs sccache: basedirs file contains a non-absolute path: $basedir" >&2
              exit 75
              ;;
          esac
          if [ -z "$runtime_basedirs" ]; then
            runtime_basedirs="$basedir"
          else
            runtime_basedirs="$runtime_basedirs:$basedir"
          fi
        done < ${lib.escapeShellArg cfg.basedirsFile}
      fi
      if [ -n "$runtime_basedirs" ]; then
        if [ -n "''${SCCACHE_BASEDIRS:-}" ]; then
          export SCCACHE_BASEDIRS="$SCCACHE_BASEDIRS:$runtime_basedirs"
        else
          export SCCACHE_BASEDIRS="$runtime_basedirs"
        fi
      fi
    ''}

    exec ${cfg.package}/bin/sccache
  '';

  wrapper = pkgs.writeShellScriptBin "sccache-rustc-wrapper" ''
    set -eu

    runtime_dir="''${XDG_RUNTIME_DIR:-/run/user/$(${pkgs.coreutils}/bin/id -u)}"
    export XDG_RUNTIME_DIR="$runtime_dir"
    if [ -z "''${DBUS_SESSION_BUS_ADDRESS:-}" ]; then
      export DBUS_SESSION_BUS_ADDRESS="unix:path=$runtime_dir/bus"
    fi
    socket_path="$runtime_dir/${sccacheDefault.userSocketRel}"
    cache_dir="''${XDG_CACHE_HOME:-$HOME/.cache}/${sccacheDefault.userCacheRel}"

    service_ready() {
      ${pkgs.systemd}/bin/systemctl --user is-active --quiet sccache-user-daemon.service \
        && [ -S "$socket_path" ]
    }

    if ! service_ready; then
      if ! ${pkgs.coreutils}/bin/timeout ${toString (sccacheDefault.startupTimeout + 5)}s \
        ${pkgs.systemd}/bin/systemctl --user start sccache-user-daemon.service; then
        echo "sccache-rustc-wrapper: managed sccache service could not start; build refused" >&2
        ${pkgs.systemd}/bin/systemctl --user status sccache-user-daemon.service --no-pager -l >&2 || true
        exit 75
      fi
    fi

    if ! service_ready; then
      echo "sccache-rustc-wrapper: managed sccache socket is not ready; build refused" >&2
      ${pkgs.systemd}/bin/journalctl --user-unit=sccache-user-daemon.service --no-pager -n 80 >&2 || true
      exit 75
    fi

    export SCCACHE_SERVER_UDS="$socket_path"
    export SCCACHE_DIR="$cache_dir"
    export CARGO_INCREMENTAL="0"
    exec ${cfg.package}/bin/sccache "$@"
  '';

  effectiveWrapper =
    if cfg.wrapperPackage != null
    then cfg.wrapperPackage
    else wrapper;

  effectiveWrapperBinary = "${effectiveWrapper}/bin/${cfg.wrapperBinaryName}";

  uid = osConfig.users.users.${config.home.username}.uid;
in {
  options.programs.harborRs.sccache.userDaemon = {
    enable = mkOption {
      type = types.bool;
      default = osSccacheEnabled;
      defaultText =
        lib.literalExpression
        "osConfig.programs.harborRs.sccache.enable";
      description = ''
        Enable a user-owned interactive sccache daemon and rustc wrapper.
        Nix sandbox builds continue to use the system harbor-rs daemon.
      '';
    };

    package = lib.mkPackageOption pkgs "sccache" {};

    wrapperPackage = mkOption {
      type = types.nullOr types.package;
      default = null;
      description = ''
        Override the sccache user-daemon client wrapper package. When null, uses
        the built-in wrapper that starts the service via systemctl --user and
        exits 75 on failure. Set to the sccache sub-flake's
        sccache-user-daemon-client for a maintained standalone version.
      '';
    };

    wrapperBinaryName = mkOption {
      type = types.str;
      default = "sccache-rustc-wrapper";
      description = ''
        Binary name inside the wrapper package. Change when using a custom
        wrapperPackage with a different entry point (e.g.
        "sccache-user-daemon-client" for the sub-flake wrapper).
      '';
    };

    idleTimeout = mkOption {
      type = types.int;
      default = sccacheDefault.idleTimeout;
      description = "Seconds before the user sccache daemon shuts down when idle. 0 = never.";
    };

    basedirs = mkOption {
      type = types.listOf types.str;
      default = [];
      description = ''
        Absolute worktree roots whose prefixes sccache removes from cache
        keys. Keep all stable workspace roots here so equivalent checkouts
        share artifacts without allowing unrelated paths to collide.
      '';
    };

    basedirsFile = mkOption {
      type = types.nullOr types.str;
      default = null;
      example = "/run/user/1000/canix/sccache-basedirs";
      description = ''
        Optional newline-delimited runtime file of absolute worktree roots.
        The daemon reads it on every start and appends it to the declarative
        basedirs list. This lets workspace managers synchronize ephemeral Git
        worktrees without putting host discovery in this generic module.
      '';
    };

    multiLevelChain = mkOption {
      type = types.nullOr types.str;
      default = null;
      example = "disk,redis,s3";
      description = ''
        Optional sccache multi-level backend chain for the interactive daemon.
        Levels are checked and populated from left to right. Use a Unix Redis
        endpoint for shared local storage; sandbox derivations never inherit
        these interactive credentials or daemon settings.
      '';
    };

    redisEndpoint = mkOption {
      type = types.nullOr types.str;
      default = null;
      example = "redis+unix://localhost/run/redis-sccache/redis.sock";
      description = "Optional Redis/Valkey endpoint for the interactive daemon.";
    };

    redisKeyPrefix = mkOption {
      type = types.nullOr types.str;
      default = null;
      example = "canix/canix-rust-v5-sccache-0.16.0";
      description = "Optional Redis/Valkey key prefix for the interactive daemon.";
    };

    redisRwMode = mkOption {
      type = types.nullOr types.str;
      default = null;
      example = "READ_WRITE";
      description = "Optional Redis read/write mode for the interactive daemon.";
    };

    multiLevelWriteErrorPolicy = mkOption {
      type = types.nullOr types.str;
      default = null;
      example = "ignore";
      description = "Optional policy for errors writing a non-primary cache level.";
    };

    environment = mkOption {
      type = types.attrsOf types.str;
      default = {};
      description = ''
        Explicit interactive-daemon environment additions. Sandbox-only
        transport variables are never inherited implicitly.
      '';
    };

    environmentFile = mkOption {
      type = types.nullOr types.str;
      default = null;
      example = "/run/user/1000/canix/sccache.env";
      description = ''
        Optional systemd EnvironmentFile for interactive-daemon credentials.
        Cache endpoint, bucket, and region remain declarative in the NixOS
        sccache module. Inline credentials remain supported.
      '';
    };
  };

  config = mkIf cfg.enable {
    assertions = [
      {
        assertion = osSccacheEnabled;
        message = ''
          programs.harborRs.sccache.userDaemon.enable requires integrated
          NixOS Home Manager with osConfig.programs.harborRs.sccache.enable = true.
        '';
      }
      {
        assertion = osSccacheRemoteEnv ? SCCACHE_BUCKET && osSccacheRemoteEnv ? SCCACHE_ENDPOINT;
        message = ''
          programs.harborRs.sccache.userDaemon.enable requires an S3-backed
          harbor-rs sccache configuration; refusing to silently fall back to
          local-only interactive caching.
        '';
      }
      {
        assertion = builtins.all (name: !(builtins.hasAttr name cfg.environment)) reservedUserEnvironment;
        message = ''
          programs.harborRs.sccache.userDaemon.environment contains a
          reserved lifecycle variable. Configure cache transport through the
          harbor-rs sccache module instead.
        '';
      }
      {
        assertion = builtins.all (lib.hasPrefix "/") cfg.basedirs;
        message = "programs.harborRs.sccache.userDaemon.basedirs entries must be absolute paths.";
      }
      {
        assertion = cfg.basedirsFile == null || lib.hasPrefix "/" cfg.basedirsFile;
        message = "programs.harborRs.sccache.userDaemon.basedirsFile must be an absolute path.";
      }
      {
        assertion = cfg.environmentFile == null || lib.hasPrefix "/" cfg.environmentFile;
        message = "programs.harborRs.sccache.userDaemon.environmentFile must be an absolute path.";
      }
      {
        assertion =
          cfg.multiLevelChain
          == null
          || builtins.all (level: builtins.elem level ["disk" "redis" "s3"])
          (lib.splitString "," cfg.multiLevelChain);
        message = "programs.harborRs.sccache.userDaemon.multiLevelChain must contain only disk, redis, and s3 levels.";
      }
      {
        assertion =
          cfg.multiLevelChain
          == null
          || !(builtins.elem "redis" (lib.splitString "," cfg.multiLevelChain))
          || cfg.redisEndpoint != null;
        message = "programs.harborRs.sccache.userDaemon.redisEndpoint is required when the multi-level chain contains redis.";
      }
      {
        assertion =
          cfg.redisEndpoint
          == null
          || cfg.redisKeyPrefix != null;
        message = "programs.harborRs.sccache.userDaemon.redisKeyPrefix is required when redisEndpoint is configured.";
      }
    ];

    home.packages = [
      cfg.package
      effectiveWrapper
    ];

    home.sessionVariables = {
      RUSTC_WRAPPER = effectiveWrapperBinary;
      SCCACHE_SERVER_UDS = "/run/user/${toString uid}/${sccacheDefault.userSocketRel}";
      CARGO_INCREMENTAL = "0";
    };

    programs.nushell.environmentVariables = {
      RUSTC_WRAPPER = effectiveWrapperBinary;
      SCCACHE_SERVER_UDS = "/run/user/${toString uid}/${sccacheDefault.userSocketRel}";
      CARGO_INCREMENTAL = "0";
    };

    systemd.user.sessionVariables.CARGO_INCREMENTAL = "0";

    systemd.user.services.sccache-user-daemon = {
      Unit = {
        Description = "User sccache compilation cache daemon";
        StartLimitIntervalSec = "${toString sccacheDefault.restartWindow}s";
        StartLimitBurst = sccacheDefault.restartBurst;
      };
      Service =
        {
          Type = "exec";
          Restart = "on-failure";
          RestartSec = "${toString sccacheDefault.restartDelay}s";
          TimeoutStartSec = "${toString sccacheDefault.startupTimeout}s";
          TimeoutStopSec = "10s";
          Environment = serviceEnvironment;
          ExecStartPre = [
            "${pkgs.coreutils}/bin/mkdir -p %t/${builtins.dirOf sccacheDefault.userSocketRel} ${config.xdg.cacheHome}/${sccacheDefault.userCacheRel}"
            "${sccacheService.repairSocket} %t/${sccacheDefault.userSocketRel}"
          ];
          ExecStart = daemonLauncher;
          ExecStartPost = "${sccacheService.waitForSocket} %t/${sccacheDefault.userSocketRel} ${toString sccacheDefault.startupTimeout}";
          ExecStop = "${pkgs.coreutils}/bin/env -u SCCACHE_START_SERVER ${cfg.package}/bin/sccache --stop-server";
          KillMode = "control-group";
        }
        // lib.optionalAttrs (cfg.environmentFile != null) {
          EnvironmentFile = cfg.environmentFile;
        };
      Install.WantedBy = ["default.target"];
    };
  };
}
