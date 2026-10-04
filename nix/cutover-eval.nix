{
  pkgs,
  module,
}: let
  inherit (pkgs) lib;
  eval = import "${pkgs.path}/nixos/lib/eval-config.nix" {
    system = pkgs.stdenv.hostPlatform.system;
    modules = [
      module
      {
        system.stateVersion = "26.05";
        networking.hostName = "fixture";
        services.harbor-db.cutover = {
          enable = true;
          operatorUsers = ["operator"];
          resources.history = {
            user = "archive";
            runtime_units = ["archive.service"];
            custody_file = "/var/lib/authority/custody.json";
            authority = {
              state_dir = "/var/lib/authority";
              directories = ["/srv/history"];
              binding.backend = "files";
            };
          };
        };
        services.harbor-db.dataDirectories = [
          {
            path = "/srv/history";
            user = "archive";
            group = "archive";
            create = false;
          }
        ];
        systemd.tmpfiles.rules = ["d '/srv/history' 0750 archive archive - -" "d \"/srv/history\" 0750 archive archive - -"];
        systemd.services.archive.serviceConfig.ExecStart = "${pkgs.coreutils}/bin/sleep infinity";
      }
    ];
  };
  inherit (eval) config;
  unsafe =
    (eval.extendModules {
      modules = [{services.harbor-db.dataDirectories = lib.mkForce [{path = "/srv/history";}];}];
    }).config;
  harborAssertions = assertions: lib.filter (item: lib.hasPrefix "Harbor-DB" item.message) assertions;
  succeeds = lib.all (item: item.assertion) (harborAssertions config.assertions);
in
  (import ./eval-checks.nix {inherit pkgs;}).mkEvalCheck {
    name = "harbor-db-cutover-eval";
    assertions = [
      {
        name = "valid-adopted-contract";
        assertion = succeeds;
        message = "existing corpus configuration must satisfy NixOS assertions";
      }
      {
        name = "adopted-roots-cannot-be-initialized";
        assertion = !(lib.all (item: item.assertion) (harborAssertions unsafe.assertions));
        message = "automatic directory creation at an adopted corpus root must fail evaluation";
      }
      {
        name = "activation-inspection-precedes-application-checks";
        assertion = config.system.preSwitchChecks ? "00---harbor-db-cutover";
        message = "raw switch-to-configuration must retain the mandatory candidate admission check";
      }
      {
        name = "startup-unit-name-is-service-normalized";
        assertion = lib.any (command: lib.hasInfix "--phase startup" command) config.systemd.services.archive.serviceConfig.ExecStartPre;
        message = "service startup must guard the source before a writer starts";
      }
      {
        name = "writer-retains-resource-lifetime-lease";
        assertion = lib.hasInfix "harbor-db-cutover serve" config.systemd.services.archive.serviceConfig.ExecStart;
        message = "the real service process must retain custody's shared authority lease";
      }
      {
        name = "activation-does-not-create-missing-history";
        assertion =
          !(lib.hasInfix "install -d" config.system.activationScripts.harbor-db-establish-data-directories.text)
          && lib.hasInfix "test -d" config.system.activationScripts.harbor-db-establish-data-directories.text
          && lib.elem "z /srv/history 0750 archive archive - -" config.systemd.tmpfiles.rules
          && lib.elem "z '/srv/history' 0750 archive archive - -" config.systemd.tmpfiles.rules
          && lib.elem "z \"/srv/history\" 0750 archive archive - -" config.systemd.tmpfiles.rules;
        message = "both live activation and boot tmpfiles must preserve require-existing roots";
      }
      {
        name = "read-only-sudo-is-exact-argv";
        assertion = lib.any (rule:
          lib.any (command:
            lib.hasSuffix "check --contract /etc/harbor-db/cutover.json --host fixture --phase preflight" command.command
            && !(lib.hasInfix "*" command.command))
          rule.commands)
        config.security.sudo.extraRules;
        message = "operator privilege must grant only the installed immutable read-only contract";
      }
    ];
    nativeBuildInputs = [pkgs.python3];
    runtimeScript = ''
      ${config.services.harbor-db.cutover.bundle}/checker --help
      ${config.services.harbor-db.cutover.bundle}/checker check --contract ${config.services.harbor-db.cutover.manifest} --host wrong > blocked.json && exit 1
      ${pkgs.python3}/bin/python3 - <<'PY'
      import json
      manifest = json.load(open('${config.services.harbor-db.cutover.manifest}'))
      assert manifest['enforced'] is True
      assert manifest['resources']['history']['authority']['directories'] == ['/srv/history']
      PY
    '';
  }
