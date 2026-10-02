{
  pkgs,
  module,
  lifecycleModule,
}: let
  inherit (pkgs) lib;
  inherit (import ./eval-checks.nix {inherit pkgs;}) mkEvalCheck;
  eval = import "${pkgs.path}/nixos/lib/eval-config.nix" {
    system = "x86_64-linux";
    modules = [
      lifecycleModule
      # Multiple consumers import the default module; its options must appear
      # once, including when the standalone lifecycle module is also imported.
      {imports = [module];}
      {imports = [module];}
      {
        system.stateVersion = "26.05";
        services.postgresql = {
          enable = true;
          package = pkgs.postgresql_18;
          dataDir = "/srv/postgres/18";
        };
        services.harbor-db.postgresql = {
          enable = true;
          stateDir = "/srv/postgres/authority";
          requiredMounts = ["/srv"];
          switchAdoption.systemIdentifier = "12345";
          recovery = {
            systemIdentifier = "12345";
            backupRoot = "/srv/backups";
            snapshotFile = "/srv/backups/records.json";
            receiptFile = "/srv/backups/recovery.json";
            offHostReceiptFile = "/srv/backups/off-host.json";
            recordChecks.records = {
              database = "app";
              sql = "SELECT count(*) FROM records";
            };
          };
          upgrade = {
            oldPackage = pkgs.postgresql_17;
            oldDataDir = "/srv/postgres/17";
            validateCommand = ["/bin/validate-upgrade"];
          };
        };
      }
    ];
  };
  preStart = eval.config.systemd.services.postgresql.preStart;
  upgrade = eval.config.systemd.services.harbor-db-postgresql-upgrade;
  withoutAdoption = eval.extendModules {
    modules = [{services.harbor-db.postgresql.switchAdoption = lib.mkForce null;}];
  };
in
  mkEvalCheck {
    name = "harbor-db-postgres-lifecycle-eval";
    resultMessage = "PostgreSQL identity guard precedes initialization and upgrade is explicit";
    assertions = [
      {
        name = "recovery-before-adoption";
        assertion = let
          checks = eval.config.system.preSwitchChecks;
          service = eval.config.systemd.services.harbor-db-postgresql-recovery-check;
        in
          lib.hasInfix "inspect-recovery" checks."00-harbor-db-postgresql-recovery"
          && service.wantedBy == []
          && service.serviceConfig.ReadWritePaths == []
          && service.serviceConfig.User == "postgres"
          && !lib.hasInfix "recovery" preStart;
        message = "Recovery admission must be read-only, precede adoption and never start a boot-time drill.";
      }
      {
        name = "no-implicit-switch-adoption";
        assertion = !(withoutAdoption.config.system.preSwitchChecks ? harbor-db-postgresql-adoption);
        message = "A guarded cluster must not gain an implicit switch-time adoption request.";
      }
      {
        name = "switch-adoption-before-unit-stop-only";
        assertion = let
          check = eval.config.system.preSwitchChecks.harbor-db-postgresql-adoption;
        in
          lib.hasInfix "runuser -u postgres --" check
          && lib.hasInfix "inspect-live --system-identifier 12345" check
          && lib.hasInfix "switch|test)" check
          && lib.hasInfix "adopt-live --system-identifier 12345" check
          && !lib.hasInfix "adopt" preStart;
        message = "Explicit switch adoption must verify the live primary before unit stop; startup cannot adopt.";
      }
      {
        name = "guard-before-initdb";
        assertion =
          lib.hasPrefix "${lib.getExe eval.config.services.harbor-db.postgresql.package} --config" preStart
          && lib.hasInfix "check\n" (builtins.head (lib.splitString "initdb" preStart));
        message = "Identity guard must run before NixOS initdb on every startup.";
      }
      {
        name = "writer-lifetime-lease";
        assertion =
          lib.hasSuffix " serve" eval.config.systemd.services.postgresql.serviceConfig.ExecStart
          && eval.config.systemd.services.postgresql.serviceConfig.NotifyAccess == "main";
        message = "The PostgreSQL process must retain the shared authority lease for its lifetime.";
      }
      {
        name = "offline-upgrade-only";
        assertion = upgrade.wantedBy == [] && lib.elem "postgresql.service" upgrade.conflicts;
        message = "Major upgrades must be explicit and stop the writer.";
      }
      {
        name = "persistent-state-mount";
        assertion = lib.elem "/srv/postgres/authority" eval.config.systemd.services.postgresql.unitConfig.RequiresMountsFor;
        message = "Identity state must be mounted before startup.";
      }
      {
        name = "durable-commit-settings";
        assertion =
          eval.config.services.postgresql.settings.fsync
          && eval.config.services.postgresql.settings.full_page_writes
          && eval.config.services.postgresql.settings.synchronous_commit == "on";
        message = "Guarded clusters must retain PostgreSQL durable-commit settings.";
      }
    ];
  }
