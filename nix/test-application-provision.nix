{
  pkgs,
  module,
  nativePackage ? import ./native-package.nix {inherit pkgs;},
  interop ? false,
  testPackage ? null,
}: let
  legacyStorage = import ./test-python-package.nix {inherit pkgs;};
  bridge = import ./test-native-bridge.nix {inherit pkgs;};
  schema = pkgs.writeText "demo-schema.sql" ''
    CREATE TABLE IF NOT EXISTS documents(id int);
    CREATE TABLE IF NOT EXISTS history(id int);
    CREATE OR REPLACE FUNCTION hidden_write() RETURNS int LANGUAGE sql SECURITY DEFINER AS $$ INSERT INTO documents VALUES (86) RETURNING id $$;
    CREATE OR REPLACE PROCEDURE hidden_procedure() LANGUAGE sql AS $$ SELECT 1 $$;
  '';
in
  assert !interop || testPackage != null;
    pkgs.testers.runNixOSTest {
      extraDriverArgs = ["--junit-xml" "junit.xml"];
      extraPythonPackages = ps:
        pkgs.lib.optional interop (bridge.package ps);
      name = "harbor-db-application-provision";
      nodes.machine = {
        pkgs,
        lib,
        ...
      }: {
        imports = [module];
        services.postgresql = {
          enable = true;
          package = pkgs.postgresql_18;
          settings.port = 5544;
          authentication = lib.mkAfter "local all all trust\nhost all all all trust";
        };
        users.users.demo = {
          isSystemUser = true;
          group = "demo";
        };
        users.groups.demo = {};
        services.harbor-db.projects.demo = {
          enable = true;
          postgres.provision = {
            enable = true;
            database = "demo";
            ownerRole = "demo_owner";
            runtimeRole = "demo_runtime";
            runtimeOsUser = "demo";
            schemaUnits = ["demo-schema.service"];
            runtimeUnits = ["demo.service"];
            tables = {
              documents = ["SELECT" "INSERT" "UPDATE" "DELETE"];
              history = ["SELECT" "INSERT"];
            };
          };
        };
        systemd.services.demo-schema = {
          serviceConfig = {
            Type = "oneshot";
            ExecStart = "${pkgs.postgresql_18}/bin/psql -p 5544 -X -w -v ON_ERROR_STOP=1 -d demo -U demo_owner -f ${schema}";
          };
        };
        services.harbor-db.projects.incomplete.postgres.provision = {
          enable = true;
          database = "incomplete";
          ownerRole = "incomplete_owner";
          runtimeRole = "incomplete_runtime";
          runtimeOsUser = "demo";
          schemaUnits = ["incomplete-schema.service"];
          runtimeUnits = ["incomplete.service"];
          tables.history = ["SELECT" "INSERT"];
        };
        systemd.services.incomplete-schema.serviceConfig = {
          Type = "oneshot";
          ExecStart = "${pkgs.postgresql_18}/bin/psql -p 5544 -X -w -v ON_ERROR_STOP=1 -d incomplete -U incomplete_owner -c 'CREATE TABLE documents(id int)'";
        };
        systemd.services.incomplete.serviceConfig = {
          Type = "oneshot";
          RemainAfterExit = true;
          ExecStart = "${pkgs.coreutils}/bin/true";
        };
        systemd.services.demo = {
          wantedBy = ["multi-user.target"];
          serviceConfig = {
            Type = "oneshot";
            User = "demo";
            RemainAfterExit = true;
            ExecStart = "${pkgs.postgresql_18}/bin/psql -p 5544 -X -w -v ON_ERROR_STOP=1 -d demo -U demo_runtime -c 'INSERT INTO documents VALUES (1); INSERT INTO history VALUES (1)'";
          };
        };
        environment.systemPackages = [pkgs.postgresql_18];
        system.extraDependencies = pkgs.lib.optional interop legacyStorage;
        system.stateVersion = "26.05";
      };
      testScript =
        if interop
        then
          bridge.script {
            fixture = "${testPackage}/bin/harbor-db-provision-fixture";
            arguments = [
              "--native-provision"
              "${nativePackage}/bin/harbor-db-provision"
              "--python-provision"
              "${legacyStorage}/bin/harbor-db-provision"
              "--psql"
              "${pkgs.postgresql_18}/bin/psql"
              "--runuser"
              "${pkgs.util-linux}/bin/runuser"
              "--systemctl"
              "${pkgs.systemd}/bin/systemctl"
            ];
            nodes = ["machine"];
            artifact = "provision-acceptance.json";
          }
        else ''
          # reboot() reconnects only when QEMU was started with reboot support.
          machine.start(allow_reboot=True)
          machine.wait_for_unit("demo.service")
          machine.fail("runuser -u demo -- psql -p 5544 -X -w -v ON_ERROR_STOP=1 -d demo -U demo_runtime -c 'UPDATE history SET id=2'")
          machine.fail("runuser -u demo -- psql -p 5544 -X -w -v ON_ERROR_STOP=1 -d demo -U demo_owner -c 'SELECT 1'")
          machine.fail("runuser -u demo -- psql -p 5544 -X -w -v ON_ERROR_STOP=1 -d demo -U postgres -c 'SELECT current_user'")
          machine.fail("runuser -u demo -- psql -p 5544 -X -w -v ON_ERROR_STOP=1 -h 127.0.0.1 -d demo -U postgres -c 'SELECT current_user'")
          machine.succeed("runuser -u postgres -- psql -p 5544 -X -w -v ON_ERROR_STOP=1 -d postgres -c 'CREATE ROLE undeclared LOGIN'")
          machine.fail("runuser -u demo -- psql -p 5544 -X -w -v ON_ERROR_STOP=1 -d demo -U undeclared -c 'SELECT current_user'")
          machine.succeed("runuser -u postgres -- psql -p 5544 -X -w -v ON_ERROR_STOP=1 -d demo -U postgres -c 'SELECT current_user'")
          machine.succeed("runuser -u postgres -- harbor-db-provision --config /etc/harbor-db/demo-provision.json check")
          machine.fail("systemctl start incomplete.service")
          machine.fail("systemctl is-active incomplete.service")
          assert machine.succeed("systemctl show incomplete-schema.service -p Result --value").strip() == "success"
          assert machine.succeed("systemctl show harbor-db-incomplete-permissions.service -p Result --value").strip() == "exit-code"
          machine.fail("runuser -u postgres -- harbor-db-provision --config /etc/harbor-db/incomplete-provision.json check")
          machine.succeed("runuser -u postgres -- psql -p 5544 -d demo -c 'GRANT UPDATE ON history TO demo_runtime'")
          machine.fail("runuser -u postgres -- harbor-db-provision --config /etc/harbor-db/demo-provision.json check")
          machine.succeed("systemctl restart harbor-db-demo-permissions.service")
          machine.fail("runuser -u demo -- ${pkgs.postgresql_18}/bin/psql -p 5544 -X -w -h /run/postgresql -d demo -U demo_runtime -c 'SELECT hidden_write()'")
          machine.fail("runuser -u demo -- ${pkgs.postgresql_18}/bin/psql -p 5544 -X -w -h /run/postgresql -d demo -U demo_runtime -c 'CALL hidden_procedure()'")
          machine.succeed("runuser -u postgres -- harbor-db-provision --config /etc/harbor-db/demo-provision.json check")
          machine.reboot()
          machine.wait_for_unit("demo.service")
        '';
    }
