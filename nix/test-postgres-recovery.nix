{pkgs}: let
  tool = import ./postgres-package.nix {inherit pkgs;};
  template = pkgs.writeText "recovery-template.json" (builtins.toJSON {
    resource = "recovery-fixture";
    data_dir = "/var/lib/postgres/18";
    state_dir = "/srv/authority";
    major = "18";
    package = toString pkgs.postgresql_18;
    required_mounts = [];
    recovery = {
      system_identifier = "12345";
      source_hostname = "primary";
      backup_root = "/srv/backup";
      snapshot_file = "/srv/backup/evidence/records.json";
      receipt_file = "/srv/backup/evidence/recovery.json";
      off_host_receipt_file = "/srv/backup/evidence/off-host.json";
      max_age_seconds = 3600;
      verify_timeout_seconds = 60;
      record_checks = [
        {
          name = "saves";
          database = "postgres";
          sql = "SELECT mutation,geometry,review,revision FROM saves ORDER BY mutation";
        }
      ];
    };
  });
  node = {pkgs, ...}: {
    virtualisation.memorySize = 1024;
    environment.systemPackages = [tool pkgs.postgresql_18 pkgs.python3 pkgs.jq];
    environment.etc."recovery-template.json".source = template;
    users.users.postgres = {
      isSystemUser = true;
      group = "postgres";
    };
    users.groups.postgres = {};
    systemd.tmpfiles.rules = [
      "d /srv/backup 0700 postgres postgres -"
      "d /srv/backup/base 0700 postgres postgres -"
      "d /srv/backup/locks 0700 postgres postgres -"
      "f /srv/backup/locks/mutate 0600 postgres postgres -"
      "d /srv/backup/evidence 0700 postgres postgres -"
      "d /srv/recovered 0700 postgres postgres -"
      "d /srv/recovery-socket 0700 postgres postgres -"
      "d /srv/authority 0700 postgres postgres -"
    ];
  };
in
  pkgs.testers.runNixOSTest {
    name = "harbor-db-postgres-recovery-acceptance";
    nodes = {
      primary = {
        imports = [node];
        # A custom dataDir is consumer-provisioned, unlike the NixOS default.
        # The hardened PostgreSQL unit binds this path before its pre-start code.
        systemd.tmpfiles.rules = ["d /var/lib/postgres/18 0700 postgres postgres -"];
        services.postgresql = {
          enable = true;
          package = pkgs.postgresql_18;
          dataDir = "/var/lib/postgres/18";
          authentication = pkgs.lib.mkBefore "local replication postgres peer\n";
        };
      };
      remote = {imports = [node];};
    };
    testScript = ''
      import json
      import shlex

      start_all()
      try:
          primary.wait_for_unit("postgresql.service")
      except Exception:
          print(primary.execute("systemctl status postgresql.service --no-pager -l; journalctl -u postgresql.service --no-pager -n 80"))
          raise
      remote.wait_for_unit("multi-user.target")
      primary.succeed("runuser -u postgres -- psql -v ON_ERROR_STOP=1 -c \"CREATE TABLE saves (mutation text PRIMARY KEY, geometry jsonb, review text, revision bigint); INSERT INTO saves VALUES ('ack-1', '{\\\"circle\\\":[10,20,30]}', 'reviewed', 42)\"")
      identifier = primary.succeed("runuser -u postgres -- psql -Atqc 'SELECT system_identifier FROM pg_control_system()'").strip()
      for host in (primary, remote):
          host.succeed(f"sed 's/12345/{identifier}/g' /etc/recovery-template.json > /srv/config.json; chown postgres:postgres /srv/config.json; chmod 0600 /srv/config.json")
      command = "runuser -u postgres -- harbor-db-postgres --config /srv/config.json"
      primary.succeed("runuser -u postgres -- pg_basebackup -h /run/postgresql -U postgres -D /srv/backup/base/base-1 -X stream --checkpoint=fast")
      stop_lsn = json.loads(primary.succeed("cat /srv/backup/base/base-1/backup_manifest"))["WAL-Ranges"][0]["End-LSN"]
      target = primary.succeed("runuser -u postgres -- psql -Atqc \"SELECT pg_create_restore_point('recovery_acceptance')\"").strip()
      primary.succeed("runuser -u postgres -- psql -Atqc 'SELECT pg_switch_wal()'")
      meta = {"backup_id": "base-1", "system_identifier": identifier, "pg_major": 18, "epoch_id": "fixture-1", "backup_stop_lsn": stop_lsn, "post_backup_lsn": target}
      primary.succeed("printf '%s\\n' " + shlex.quote(json.dumps(meta)) + " > /srv/backup/base/base-1.meta.json; printf 'base-1\\n' > /srv/backup/LAST_SUCCESS; chown -R postgres:postgres /srv/backup")
      primary.fail(f"{command} inspect-recovery")
      primary.fail(f"{command} adopt-live --system-identifier {identifier}")
      primary.succeed("test ! -e /srv/authority/identity.json; test ! -e /srv/authority/lock")
      primary.succeed(f"{command} snapshot-records --socket-dir /run/postgresql --port 5432")
      # Copy completed WAL into the disposable restore only, never modify the
      # verified base backup. Reaching target is verified through SQL below.
      primary.succeed("runuser -u postgres -- cp -a /srv/backup/base/base-1 /srv/recovered/18; cp /var/lib/postgres/18/pg_wal/0000000* /srv/recovered/18/pg_wal/; chown -R postgres:postgres /srv/recovered")
      recovery_config = "listen_addresses = 'localhost'\nunix_socket_directories = '/srv/recovery-socket'\nport = 55432\nrecovery_target_lsn = '" + target + "'\nrecovery_target_action = 'promote'\ndefault_transaction_read_only = on\n"
      primary.succeed("printf '%s' " + shlex.quote(recovery_config) + " > /srv/recovered/18/postgresql.conf; touch /srv/recovered/18/recovery.signal; chown postgres:postgres /srv/recovered/18/postgresql.conf /srv/recovered/18/recovery.signal")
      primary.succeed("tar -C /srv -cf /tmp/recovery.tar backup recovered")
      primary.succeed("runuser -u postgres -- pg_ctl -D /srv/recovered/18 -l /srv/recovered/server.log -w start")
      primary.wait_until_succeeds("runuser -u postgres -- psql -h /srv/recovery-socket -p 55432 -Atqc 'SELECT NOT pg_is_in_recovery()' | grep -qx t")
      primary.succeed(f"{command} certify-recovery --data-dir /srv/recovered/18 --socket-dir /srv/recovery-socket --port 55432")
      primary.fail(f"{command} inspect-recovery")  # Missing independent off-host execution.

      # Execute against the same restored bytes on a second, independently
      # named host, rather than copying the primary's local success receipt.
      primary.succeed("runuser -u postgres -- pg_ctl -D /srv/recovered/18 -w stop")
      # The driver's target directory is relative to its retained output and
      # shared transport, not an arbitrary absolute host temporary directory.
      primary.copy_from_machine("/tmp/recovery.tar", "recovery-transfer")
      remote.copy_from_host(str(primary.out_dir / "recovery-transfer/recovery.tar"), "/tmp/recovery.tar")
      remote.succeed("tar -C /srv -xf /tmp/recovery.tar; chown -R postgres:postgres /srv/backup /srv/recovered")
      remote.succeed("runuser -u postgres -- pg_ctl -D /srv/recovered/18 -l /srv/recovered/remote.log -w start")
      remote.wait_until_succeeds("runuser -u postgres -- psql -h /srv/recovery-socket -p 55432 -Atqc 'SELECT NOT pg_is_in_recovery()' | grep -qx t")
      remote.succeed("jq '.recovery.receipt_file = .recovery.off_host_receipt_file' /srv/config.json > /srv/remote-config.json; chown postgres:postgres /srv/remote-config.json")
      remote.succeed("runuser -u postgres -- harbor-db-postgres --config /srv/remote-config.json certify-recovery --data-dir /srv/recovered/18 --socket-dir /srv/recovery-socket --port 55432")
      remote.copy_from_machine("/srv/backup/evidence/off-host.json", "recovery-transfer")
      primary.copy_from_host(str(remote.out_dir / "recovery-transfer/off-host.json"), "/srv/backup/evidence/off-host.json")
      primary.succeed("chown postgres:postgres /srv/backup/evidence/off-host.json")
      primary.succeed(f"{command} inspect-recovery")
      primary.succeed(f"{command} adopt-live --system-identifier {identifier}")
      primary.succeed("test -s /srv/authority/identity.json")
      # Same table count cannot hide a changed review, and a corrupt retained
      # backup cannot reuse a formerly successful recovery receipt.
      remote.succeed("runuser -u postgres -- psql -h /srv/recovery-socket -p 55432 -v ON_ERROR_STOP=1 -c \"BEGIN READ WRITE; UPDATE saves SET review = 'lost-review'; COMMIT\"")
      remote.fail("runuser -u postgres -- harbor-db-postgres --config /srv/remote-config.json certify-recovery --data-dir /srv/recovered/18 --socket-dir /srv/recovery-socket --port 55432")
      primary.succeed("printf corrupt >> /srv/backup/base/base-1/PG_VERSION")
      primary.fail(f"{command} inspect-recovery")
    '';
  }
