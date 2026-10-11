{pkgs}: let
  inherit (pkgs) lib;
  inherit (import ./eval-checks.nix {inherit pkgs;}) mkEvalCheck;
  eval = import "${pkgs.path}/nixos/lib/eval-config.nix" {
    system = "x86_64-linux";
    modules = [
      ./pg-backup.nix
      {
        system.stateVersion = "24.11";
        services.harbor-db.pgBackup = {
          enable = true;
          role = "target";
          source.hostName = "10.0.0.2";
          targetSettings.replicatorPasswordFile = "/run/secrets/pg-replicator-password";
        };
      }
    ];
  };
  sourceEval = import "${pkgs.path}/nixos/lib/eval-config.nix" {
    system = "x86_64-linux";
    modules = [
      ./pg-backup.nix
      {
        system.stateVersion = "24.11";
        services.postgresql.enable = true;
        services.harbor-db.pgBackup = {
          enable = true;
          role = "source";
          source.hostName = "10.0.0.1";
          sourceSettings = {
            listenAddresses = ["10.0.0.1"];
            allowedReplicationHosts = ["10.0.0.2/32"];
            replicatorPasswordFile = "/run/secrets/pg-replicator-password";
          };
        };
      }
    ];
  };
  receive = eval.config.systemd.services.pg-receivewal.serviceConfig;
  base = eval.config.systemd.services.pg-basebackup.script;
  sourcePostgresql = sourceEval.config.services.postgresql;
  localEval = import "${pkgs.path}/nixos/lib/eval-config.nix" {
    system = "x86_64-linux";
    modules = [
      ./pg-backup.nix
      {
        system.stateVersion = "24.11";
        services.postgresql.enable = true;
        services.harbor-db.pgBackup = {
          enable = true;
          role = "both";
          source.hostName = "127.0.0.1";
          sourceSettings = {
            replicatorPasswordFile = "/run/secrets/pg-replicator-password";
          };
          targetSettings.sourceLocalRecovery.enable = true;
        };
      }
    ];
  };
  localBackup = localEval.config.systemd.services.pg-basebackup.script;
  ipv6Local =
    (localEval.extendModules {
      modules = [{services.harbor-db.pgBackup.source.hostName = lib.mkForce "::1";}];
    }).config;
  localhost =
    (localEval.extendModules {
      modules = [{services.harbor-db.pgBackup.source.hostName = lib.mkForce "localhost";}];
    }).config;
  explicitLocal = host:
    (localEval.extendModules {
      modules = [
        {
          services.harbor-db.pgBackup.source.hostName = lib.mkForce host;
          services.harbor-db.pgBackup.sourceSettings.listenAddresses = ["10.0.0.1"];
        }
      ];
    }).config;
  fencedLocal =
    (localEval.extendModules {
      modules = [./postgres-lifecycle.nix {services.harbor-db.postgresql.enable = true;}];
    }).config;
  blockedFence =
    (localEval.extendModules {
      modules = [
        ./postgres-lifecycle.nix
        {
          services.harbor-db.postgresql = {
            enable = true;
            writerFence.replicationRoles = lib.mkForce [];
          };
        }
      ];
    }).config;
  explicitFence =
    (localEval.extendModules {
      modules = [
        ./postgres-lifecycle.nix
        {
          services.harbor-db.postgresql = {
            enable = true;
            writerFence.replicationRoles = ["peer" "replicator"];
          };
        }
      ];
    }).config;
  walSizeAccepted = size: let
    selected = eval.extendModules {
      modules = [{services.harbor-db.pgBackup.targetSettings.walSegmentBytes = lib.mkForce size;}];
    };
  in
    (builtins.tryEval (builtins.deepSeq selected.config.systemd.services.pg-backup-prune.script true)).success;
in
  mkEvalCheck {
    name = "harbor-db-pg-backup-eval";
    resultMessage = "harbor-db PostgreSQL backup creates slots before streaming and verifies base backups";
    assertions = [
      {
        name = "source-local-recovery-requires-base-backup-production";
        message = "Source-local recovery requires both fresh verified base backups and continuous WAL; independent WAL-only targets remain supported.";
        assertion = let
          selected = (localEval.extendModules {modules = [{services.harbor-db.pgBackup.targetSettings.baseBackup.enable = false;}];}).config;
          walOnlyTarget = (eval.extendModules {modules = [{services.harbor-db.pgBackup.targetSettings.baseBackup.enable = false;}];}).config;
        in
          lib.any (a: !a.assertion && lib.hasInfix "sourceLocalRecovery requires baseBackup" a.message) selected.assertions
          && lib.all (a: a.assertion || !lib.hasPrefix "services.harbor-db.pgBackup" a.message) localEval.config.assertions
          && lib.all (a: a.assertion || !lib.hasPrefix "services.harbor-db.pgBackup" a.message) walOnlyTarget.assertions
          && localEval.config.systemd.services ? pg-basebackup
          && !(walOnlyTarget.systemd.services ? pg-basebackup);
      }
      {
        name = "invalid-wal-segment-size-rejects-before-backup-services";
        assertion = lib.all (size: !walSizeAccepted size) [0 3 524288 1048577 3145728 1073741825 2147483648];
        message = "WAL segment sizes must reject before generating a prune service that blocks base backups.";
      }
      {
        name = "all-runtime-supported-wal-segment-sizes-evaluate";
        assertion = lib.all walSizeAccepted [1048576 2097152 4194304 8388608 16777216 33554432 67108864 134217728 268435456 536870912 1073741824];
        message = "Every supported power-of-two WAL segment size must remain valid.";
      }
      {
        name = "source-local-recovery-requires-wal-receiver";
        assertion = let
          blocked = (localEval.extendModules {modules = [{services.harbor-db.pgBackup.targetSettings.receiveWal.enable = false;}];}).config;
        in
          lib.any (a: !a.assertion && lib.hasInfix "sourceLocalRecovery requires receiveWal" a.message) blocked.assertions;
        message = "Base backups alone cannot establish source-local post-target WAL coverage.";
      }
      {
        name = "source-local-fence-admits-generated-receiver";
        assertion = lib.elem "replicator" fencedLocal.services.harbor-db.postgresql.writerFence.replicationRoles;
        message = "The generated source-local WAL receiver must reconnect under the default writer fence.";
      }
      {
        name = "source-local-fence-preserves-peer-roles-without-duplicates";
        assertion = explicitFence.services.harbor-db.postgresql.writerFence.replicationRoles == ["peer" "replicator"];
        message = "Automatic receiver enrollment must preserve explicitly configured replication peers and avoid duplicate fence roles.";
      }
      {
        name = "source-local-forced-empty-fence-rejected";
        assertion = lib.any (a: !a.assertion && lib.hasInfix "replicator" a.message) blockedFence.assertions;
        message = "An explicit forced fence override must not silently disconnect source-local recovery.";
      }
      {
        name = "flush-wal-on-receipt";
        assertion = lib.hasInfix "--synchronous" receive.ExecStart;
        message = "WAL must be flushed on receipt, including partial segments.";
      }
      {
        name = "slot-create-is-pre-start";
        assertion = lib.hasInfix "--create-slot" (lib.concatStringsSep "\n" receive.ExecStartPre) && !(lib.hasInfix "--create-slot" receive.ExecStart);
        message = "pg_receivewal slot creation must be a one-shot ExecStartPre";
      }
      {
        name = "restart-policy";
        assertion = receive.Restart == "on-failure" && receive.RestartSteps == 6 && receive.RestartMaxDelaySec == "5min";
        message = "continuous WAL streaming must restart after failure";
      }
      {
        name = "source-replication-hba";
        assertion = lib.hasInfix "host replication replicator 10.0.0.2/32 scram-sha-256" sourcePostgresql.authentication;
        message = "source role must render the allowed replication host in pg_hba.conf";
      }
      {
        name = "source-replication-firewall";
        assertion = lib.elem 5432 sourceEval.config.networking.firewall.interfaces.wg-home.allowedTCPPorts;
        message = "source role must open PostgreSQL on the configured replication interface";
      }
      {
        name = "verify-backup";
        assertion = lib.hasInfix "pg_verifybackup" base;
        message = "base backups must pass pg_verifybackup before publication";
      }
      {
        name = "partial-publish";
        assertion = lib.hasInfix ".partial" base && lib.hasInfix "publish-tree \"$partial_dir\" \"$date_dir\"" base;
        message = "base backups must publish through a partial directory and atomic rename";
      }
      {
        name = "source-local-both-roles";
        assertion = !(builtins.any (a: lib.hasPrefix "services.harbor-db.pgBackup" a.message) (builtins.filter (a: !a.assertion) localEval.config.assertions)) && lib.hasInfix "host replication replicator 127.0.0.1/32 scram-sha-256" localEval.config.services.postgresql.authentication && lib.hasInfix "-h 127.0.0.1" localEval.config.systemd.services.pg-receivewal.serviceConfig.ExecStart && !(localEval.config.networking.firewall.interfaces ? wg-home);
        message = "Source-local reception must configure both source credentials and generated receiver services without opening a replication firewall.";
      }
      {
        name = "source-local-ipv6-replication-hba";
        assertion = lib.hasInfix "host replication replicator ::1/128 scram-sha-256" ipv6Local.services.postgresql.authentication;
        message = "IPv6 loopback reception must have a generated authenticated replication rule.";
      }
      {
        name = "source-local-localhost-replication-hba";
        assertion = lib.all (host: lib.hasInfix "host replication replicator ${host} scram-sha-256" localhost.services.postgresql.authentication) ["127.0.0.1/32" "::1/128"];
        message = "localhost reception must admit both loopback address families.";
      }
      {
        name = "source-local-default-loopback-listeners";
        assertion = localEval.config.services.postgresql.settings.listen_addresses == "127.0.0.1" && ipv6Local.services.postgresql.settings.listen_addresses == "::1" && localhost.services.postgresql.settings.listen_addresses == "127.0.0.1,::1";
        message = "Default source-local receivers must have reachable TCP listeners for their selected loopback address families.";
      }
      {
        name = "explicit-source-listener-preserved";
        assertion = sourcePostgresql.settings.listen_addresses == "10.0.0.1";
        message = "Explicit replication listeners must remain unchanged.";
      }
      {
        name = "explicit-source-local-listeners-include-selected-loopback";
        assertion =
          (explicitLocal "127.0.0.1").services.postgresql.settings.listen_addresses
          == "10.0.0.1,127.0.0.1"
          && (explicitLocal "::1").services.postgresql.settings.listen_addresses == "10.0.0.1,::1"
          && (explicitLocal "localhost").services.postgresql.settings.listen_addresses == "10.0.0.1,127.0.0.1,::1";
        message = "An explicit replication listener must also bind the selected source-local loopback families.";
      }
      {
        name = "source-local-persistent-mutation-before-publication";
        assertion = lib.hasInfix ''exec 8<>"$backup_root/locks/mutate"'' localBackup && lib.hasInfix ''flock -n 8'' localBackup && !(lib.hasInfix ''locks/mutate'' base);
        message = "Opt-in backup publication must own the existing recovery mutation inode before its legacy anchor; legacy publication remains unchanged.";
      }
      {
        name = "source-local-private-recovery-namespace";
        assertion = lib.elem "f /var/backups/pgbackup/127.0.0.1/recovery/PROTOCOL 0600 postgres postgres - source-local-v1" localEval.config.systemd.tmpfiles.rules && lib.elem "f /var/backups/pgbackup/127.0.0.1/locks/mutate 0600 postgres postgres -" localEval.config.systemd.tmpfiles.rules;
        message = "The capture namespace and mutation inode must be provisioned outside completed backup directories with private ownership.";
      }
    ];
  }
