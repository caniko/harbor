{pkgs}: let
  tool = import ./postgres-package.nix {inherit pkgs;};
  sql = pkgs.writeText "acknowledged-save.sql" ''
    CREATE TABLE IF NOT EXISTS saves (
      mutation text PRIMARY KEY, geometry jsonb NOT NULL, review text NOT NULL,
      binding text NOT NULL, receipt text NOT NULL, revision bigint NOT NULL
    );
    BEGIN;
    INSERT INTO saves VALUES ('ack-1', '{"circle":[10,20,30]}', 'reviewed',
      'recording-1', 'receipt-1', 42) ON CONFLICT DO NOTHING;
    COMMIT;
  '';
in
  pkgs.testers.runNixOSTest {
    name = "harbor-db-postgres-crash-rollback";
    nodes.machine = {
      imports = [./postgres-lifecycle.nix];
      virtualisation.memorySize = 1024;
      services.postgresql = {
        enable = true;
        package = pkgs.postgresql_18;
        dataDir = "/var/lib/postgres/18";
      };
      services.harbor-db.postgresql.enable = true;
      environment.systemPackages = [tool pkgs.postgresql_18];
      specialisation.compat.configuration.services.postgresql.settings.track_io_timing = true;
    };
    testScript = ''
      start_all()
      machine.wait_for_unit("multi-user.target")
      # First provisioning is explicit. Normal boot cannot adopt an empty DB.
      machine.fail("systemctl start postgresql")
      machine.succeed("test ! -e /var/lib/postgres/18/PG_VERSION")
      machine.succeed("systemctl stop postgresql")
      machine.succeed("install -d -o postgres -g postgres -m 0700 /var/lib/postgres/18")
      machine.succeed("runuser -u postgres -- initdb -D /var/lib/postgres/18")
      identifier = machine.succeed("runuser -u postgres -- pg_controldata /var/lib/postgres/18 | sed -n 's/^Database system identifier: *//p'").strip()
      machine.succeed(f"runuser -u postgres -- harbor-db-postgres --config /etc/harbor-db/postgresql.json adopt --system-identifier {identifier}")
      machine.succeed("systemctl reset-failed postgresql; systemctl start postgresql")
      machine.wait_for_unit("postgresql.service")
      # The postmaster itself is MAINPID and retains the shared authority lease.
      # This verifies the real package does not close it during initialization.
      machine.succeed('test "$(systemctl show postgresql -p MainPID --value)" = "$(head -1 /var/lib/postgres/18/postmaster.pid)"')
      machine.succeed('pid=$(head -1 /var/lib/postgres/18/postmaster.pid); ls -l /proc/$pid/fd | grep -F /var/lib/harbor-db/postgresql/lock')
      machine.fail("runuser -u postgres -- flock -n -x /var/lib/harbor-db/postgresql/lock true")
      machine.succeed("systemctl reload postgresql")
      machine.fail(f"runuser -u postgres -- harbor-db-postgres --config /etc/harbor-db/postgresql.json adopt --system-identifier {identifier}")
      machine.succeed("runuser -u postgres -- psql -v ON_ERROR_STOP=1 -f ${sql}")
      # Persistent ALTER SYSTEM values must not weaken the launcher's contract.
      machine.succeed("runuser -u postgres -- psql -c 'ALTER SYSTEM SET fsync = off'")
      machine.succeed("runuser -u postgres -- psql -c 'ALTER SYSTEM SET full_page_writes = off'")
      machine.succeed("runuser -u postgres -- psql -c 'ALTER SYSTEM SET synchronous_commit = off'")
      machine.succeed("systemctl restart postgresql")
      machine.wait_for_unit("postgresql.service")
      settings = machine.succeed("runuser -u postgres -- psql -Atqc \"SELECT name,setting,source FROM pg_settings WHERE name IN ('fsync','full_page_writes','synchronous_commit') ORDER BY name\"").strip()
      assert settings.splitlines() == ['fsync|on|command line', 'full_page_writes|on|command line', 'synchronous_commit|on|command line'], settings

      def verify_save():
          result = machine.succeed("runuser -u postgres -- psql -Atqc \"SELECT mutation,geometry->'circle',review,binding,receipt,revision FROM saves\"").strip()
          assert result == 'ack-1|[10, 20, 30]|reviewed|recording-1|receipt-1|42', result

      verify_save()
      # SIGKILL the server, then abruptly terminate the VM without shutdown.
      machine.succeed("systemctl kill --signal=SIGKILL --kill-whom=all postgresql.service")
      machine.wait_for_unit("postgresql.service")
      machine.wait_until_succeeds("runuser -u postgres -- psql -Atqc 'SELECT count(*) FROM saves'")
      verify_save()
      machine.crash()
      machine.start()
      machine.wait_for_unit("postgresql.service")
      verify_save()

      # Switch to a rollback-compatible generation after an acknowledged write.
      machine.succeed("/run/current-system/specialisation/compat/bin/switch-to-configuration test")
      machine.wait_for_unit("postgresql.service")
      verify_save()
      machine.succeed("systemctl stop postgresql")
      machine.succeed("runuser -u postgres -- flock -n -x /var/lib/harbor-db/postgresql/lock true")
      # Missing storage must not be turned into a fresh cluster by NixOS.
      machine.succeed("mv /var/lib/postgres/18 /var/lib/postgres/preserved; mkdir /var/lib/postgres/18; chown postgres:postgres /var/lib/postgres/18")
      machine.fail("systemctl start postgresql")
      machine.succeed("test ! -e /var/lib/postgres/18/PG_VERSION")
      # Stop Restart=always before restoring the directory: a restarting unit's
      # ReadWritePaths bind mount could otherwise capture the empty old inode.
      machine.succeed("systemctl stop postgresql")
      machine.succeed("rmdir /var/lib/postgres/18; mv /var/lib/postgres/preserved /var/lib/postgres/18")
      machine.succeed("systemctl reset-failed postgresql; systemctl start postgresql")
      verify_save()
    '';
  }
