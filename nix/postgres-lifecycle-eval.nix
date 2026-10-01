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
in
  mkEvalCheck {
    name = "harbor-db-postgres-lifecycle-eval";
    resultMessage = "PostgreSQL identity guard precedes initialization and upgrade is explicit";
    assertions = [
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
