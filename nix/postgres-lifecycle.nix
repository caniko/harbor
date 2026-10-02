{
  config,
  lib,
  pkgs,
  ...
}: let
  inherit (lib) mkEnableOption mkIf mkOption types;
  cfg = config.services.harbor-db.postgresql;
  pg = config.services.postgresql;
  manifest = pkgs.writeText "harbor-db-postgresql.json" (builtins.toJSON ({
      inherit (cfg) resource;
      data_dir = pg.dataDir;
      major = lib.versions.major pg.finalPackage.version;
      package = toString pg.finalPackage;
      state_dir = cfg.stateDir;
      required_mounts = cfg.requiredMounts;
    }
    // lib.optionalAttrs (cfg.recovery != null) {
      recovery = {
        system_identifier = cfg.recovery.systemIdentifier;
        backup_root = cfg.recovery.backupRoot;
        snapshot_file = cfg.recovery.snapshotFile;
        receipt_file = cfg.recovery.receiptFile;
        off_host_receipt_file = cfg.recovery.offHostReceiptFile;
        source_hostname = cfg.recovery.sourceHostname;
        max_age_seconds = cfg.recovery.maxAgeSeconds;
        verify_timeout_seconds = cfg.recovery.verifyTimeoutSeconds;
        record_checks =
          lib.mapAttrsToList (name: check: {
            inherit name;
            inherit (check) database sql;
          })
          cfg.recovery.recordChecks;
      };
    }
    // lib.optionalAttrs (cfg.upgrade != null) {
      upgrade = {
        data_dir = cfg.upgrade.oldDataDir;
        major = lib.versions.major cfg.upgrade.oldPackage.version;
        package = toString cfg.upgrade.oldPackage;
        initdb_args = cfg.upgrade.initdbArgs;
        extra_config = cfg.upgrade.extraConfig;
        validate_command = cfg.upgrade.validateCommand;
        copy_command = ["${pkgs.coreutils}/bin/cp" "-aL" "--reflink=auto"];
      };
    }));
  command = "${lib.getExe cfg.package} --config ${manifest}";
  liveArgs = lib.optionalString (cfg.switchAdoption != null) (lib.escapeShellArgs [
    "--system-identifier"
    cfg.switchAdoption.systemIdentifier
    "--socket-dir"
    cfg.switchAdoption.socketDir
    "--port"
    (toString cfg.switchAdoption.port)
  ]);
  serviceUserCommand = "${pkgs.util-linux}/bin/runuser -u postgres -- ${command}";
in {
  options.services.harbor-db.postgresql = {
    enable = mkEnableOption "adopted PostgreSQL identity guards and staged upgrades";
    package = mkOption {
      type = types.package;
      default = import ./postgres-package.nix {inherit pkgs;};
      description = "Harbor DB PostgreSQL lifecycle adapter.";
    };
    resource = mkOption {
      type = types.str;
      default = "postgresql";
      description = "Stable cluster authority name, shared by supported generations.";
    };
    stateDir = mkOption {
      type = types.str;
      default = "/var/lib/harbor-db/postgresql";
      description = "Persistent identity/journal storage outside the cluster and Nix generations. Must be backed up with the cluster.";
    };
    requiredMounts = mkOption {
      type = types.listOf types.str;
      default = [];
      description = "Exact mountpoints that must exist before adoption, checking or upgrade.";
    };
    recovery = mkOption {
      default = null;
      description = ''
        Executed backup and record-level recovery acceptance required before
        adoption and each activating rollout. Startup never runs a recovery drill.
        Certification compares a disposable read-only restore with a source snapshot
        captured in the consumer's consistency window. Off-host acceptance, when
        configured, must certify the same backup and records on another host.
      '';
      type = types.nullOr (types.submodule {
        options = {
          systemIdentifier = mkOption {type = types.strMatching "[1-9][0-9]*";};
          backupRoot = mkOption {type = types.strMatching "/.*";};
          snapshotFile = mkOption {type = types.strMatching "/.*";};
          receiptFile = mkOption {type = types.strMatching "/.*";};
          offHostReceiptFile = mkOption {
            type = types.nullOr (types.strMatching "/.*");
            default = null;
            description = "Independent off-host restore receipt; null explicitly selects local-only qualification.";
          };
          sourceHostname = mkOption {
            type = types.nonEmptyStr;
            default = config.networking.hostName;
          };
          maxAgeSeconds = mkOption {
            type = types.ints.positive;
            default = 172800;
          };
          verifyTimeoutSeconds = mkOption {
            type = types.ints.positive;
            default = 900;
          };
          recordChecks = mkOption {
            type = types.attrsOf (types.submodule {
              options = {
                database = mkOption {type = types.nonEmptyStr;};
                sql = mkOption {
                  type = types.nonEmptyStr;
                  description = "Deterministic record-level SQL; source and restored results must match exactly.";
                };
              };
            });
            description = "Consumer-owned application checks, not merely database/table presence.";
          };
        };
      });
    };
    switchAdoption = mkOption {
      default = null;
      description = ''
        Explicit first-rollout adoption through NixOS switch/test pre-switch checks.
        The live local primary, physical control file and independently recorded
        identifier must agree, with all durability settings enabled. Boot and
        dry/check actions inspect without adoption; normal service startup never
        adopts. Enable only after consumer backup/record acceptance, and retire
        the request from configuration after the rollout.
      '';
      type = types.nullOr (types.submodule {
        options = {
          systemIdentifier = mkOption {
            type = types.strMatching "[1-9][0-9]*";
            description = "Independently recorded authoritative PostgreSQL system identifier.";
          };
          socketDir = mkOption {
            type = types.strMatching "/[^,]*";
            default = "/run/postgresql";
            description = "Local Unix socket directory of the existing authoritative primary.";
          };
          port = mkOption {
            type = types.port;
            default = 5432;
            description = "Port suffix of the local PostgreSQL socket.";
          };
        };
      });
    };
    upgrade = mkOption {
      default = null;
      description = "Explicit offline copy upgrade contract. Never runs automatically at boot.";
      type = types.nullOr (types.submodule {
        options = {
          oldPackage = mkOption {
            type = types.package;
            description = "Old PostgreSQL package including its extensions.";
          };
          oldDataDir = mkOption {
            type = types.str;
            description = "Adopted old cluster directory.";
          };
          initdbArgs = mkOption {
            type = types.listOf types.str;
            default = [];
            description = "Explicit locale, encoding and checksum arguments matching the old cluster.";
          };
          extraConfig = mkOption {
            type = types.lines;
            default = "";
            description = "Configuration required while pg_upgrade starts the new server.";
          };
          validateCommand = mkOption {
            type = types.listOf types.str;
            description = "Validation argv. Receives the offline staging directory as its final argument; must stop any server it starts before returning.";
          };
        };
      });
    };
  };

  config = mkIf cfg.enable {
    assertions = [
      {
        assertion = pg.enable;
        message = "services.harbor-db.postgresql requires services.postgresql.enable.";
      }
      {
        assertion = cfg.stateDir != pg.dataDir && !(lib.hasPrefix "${pg.dataDir}/" cfg.stateDir);
        message = "Harbor DB identity state must be outside the PostgreSQL data directory.";
      }
      {
        assertion = cfg.upgrade == null || cfg.upgrade.validateCommand != [];
        message = "Staged PostgreSQL upgrades require consumer validation.";
      }
      {
        assertion = cfg.recovery == null || cfg.recovery.recordChecks != {};
        message = "PostgreSQL recovery admission requires consumer record-level checks.";
      }
      {
        assertion = cfg.recovery == null || cfg.switchAdoption == null || cfg.recovery.systemIdentifier == cfg.switchAdoption.systemIdentifier;
        message = "Recovery and switch adoption must name the same authoritative cluster.";
      }
    ];
    environment.systemPackages = [cfg.package];
    environment.etc."harbor-db/postgresql.json".source = manifest;
    systemd.tmpfiles.rules = ["d ${cfg.stateDir} 0700 postgres postgres -"];
    system.preSwitchChecks = lib.mkMerge [
      (lib.mkIf (cfg.recovery != null) {
        # Lexical ordering rejects missing recovery evidence before any adoption.
        "00-harbor-db-postgresql-recovery" = ''
          ${serviceUserCommand} inspect-recovery
        '';
      })
      (lib.mkIf (cfg.switchAdoption != null) {
        harbor-db-postgresql-adoption = ''
          # Verify while the old primary is still running, before NixOS stops units.
          ${serviceUserCommand} inspect-live ${liveArgs}
          case "''${2-}" in
            switch|test)
              ${pkgs.coreutils}/bin/install -d -m 0700 -o postgres -g postgres ${lib.escapeShellArg cfg.stateDir}
              ${serviceUserCommand} adopt-live ${liveArgs}
              ;;
          esac
        '';
      })
    ];

    systemd.services.harbor-db-postgresql-recovery-check = mkIf (cfg.recovery != null) {
      description = "Read-only PostgreSQL backup and recovery admission";
      unitConfig.RequiresMountsFor = [cfg.recovery.backupRoot];
      serviceConfig = {
        Type = "oneshot";
        User = "postgres";
        Group = "postgres";
        ExecStart = "${command} inspect-recovery";
        TimeoutStartSec = cfg.recovery.verifyTimeoutSeconds + 60;
        ProtectSystem = "strict";
        ReadWritePaths = [];
        PrivateTmp = true;
        PrivateNetwork = true;
        NoNewPrivileges = true;
      };
    };

    # Runs on EVERY start, in the same unit as nixpkgs' initialization code.
    # A failed guard exits before initdb can turn missing storage into an empty DB.
    systemd.services.postgresql = {
      preStart = lib.mkBefore ''
        ${command} check
      '';
      unitConfig.RequiresMountsFor = [cfg.stateDir] ++ cfg.requiredMounts;
      serviceConfig = {
        ExecStart = lib.mkForce "${command} serve";
        # serve execs postgres: MAINPID owns the lease, readiness and signals.
        NotifyAccess = "main";
      };
    };
    services.postgresql.settings = {
      fsync = lib.mkForce true;
      full_page_writes = lib.mkForce true;
      synchronous_commit = lib.mkForce "on";
    };

    systemd.services.harbor-db-postgresql-upgrade = mkIf (cfg.upgrade != null) {
      description = "Explicit staged PostgreSQL major upgrade";
      conflicts = ["postgresql.service"];
      after = ["postgresql.service"];
      unitConfig.RequiresMountsFor = [cfg.stateDir pg.dataDir cfg.upgrade.oldDataDir] ++ cfg.requiredMounts;
      serviceConfig = {
        Type = "oneshot";
        User = "postgres";
        Group = "postgres";
        ExecStart = "${command} upgrade";
        TimeoutStartSec = "infinity";
        UMask = "0077";
        PrivateTmp = true;
        PrivateNetwork = true;
        NoNewPrivileges = true;
        ProtectSystem = "strict";
        # Staging/publication need the destination parent, including on first upgrade.
        ReadWritePaths = [cfg.stateDir (builtins.dirOf pg.dataDir) cfg.upgrade.oldDataDir];
      };
      environment.LOCALE_ARCHIVE = "${pkgs.glibcLocales}/lib/locale/locale-archive";
    };
  };
}
