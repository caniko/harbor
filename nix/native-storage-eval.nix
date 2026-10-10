{
  pkgs,
  module,
  defaultModule ? module,
  nativePackage,
}: let
  backupCommands =
    pkgs.lib.genAttrs ["capture" "restore" "verify" "cleanup"] (stage:
      ["${pkgs.coreutils}/bin/true" "{backup}"] ++ pkgs.lib.optional (stage != "capture") "{workspace}");
  adapterFixture = {config, ...}: {
    imports = [./pg-backup.nix];
    system.stateVersion = "26.05";
    services.postgresql = {
      enable = true;
      package = pkgs.postgresql_18;
      dataDir = "/var/lib/postgresql/18";
    };
    services.harbor-db.postgresql.enable = true;
    services.harbor-db.cutover.enable = true;
    services.harbor-db.pgBackup = {
      enable = true;
      role = "target";
      source.hostName = "127.0.0.1";
      targetSettings.replicatorPasswordFile = "/run/fixture-password";
    };
    services.harbor-db.projects.fixture = {
      postgres.provision = {
        enable = true;
        database = "fixture";
        ownerRole = "fixture_owner";
        runtimeRole = "fixture_runtime";
        runtimeOsUser = "fixture";
      };
      backup = {
        enable = true;
        user = "fixture";
        group = "fixture";
        directory = "/var/lib/fixture-backup";
        commands = backupCommands;
      };
      transition = {
        enable = true;
        sourceManifest = toString (pkgs.writeText "source-authority.json" "{}");
        targetManifest = toString (pkgs.writeText "target-authority.json" "{}");
        backupManifest = toString config.environment.etc."harbor-db/fixture-backup.json".source;
        independentReceipt = "/var/lib/fixture-authority/independent.json";
        runtimeUnits = ["fixture.service"];
        commands = pkgs.lib.genAttrs ["import" "verify-target" "verify-source" "health"] (_: {
          user = "fixture";
          argv = ["${pkgs.coreutils}/bin/true"];
        });
      };
    };
  };
  evaluate = modules:
    import "${pkgs.path}/nixos/lib/eval-config.nix" {
      inherit (pkgs) system;
      modules = modules ++ [adapterFixture];
    };
  eval = evaluate [module];
  defaultEval = evaluate [defaultModule];
  backupNameAccepted = name: let
    configured = evaluate [
      module
      {
        services.harbor-db.projects.${name}.backup = {
          enable = true;
          user = "fixture";
          group = "fixture";
          directory = "/var/lib/fixture-backup";
          commands = backupCommands;
        };
      }
    ];
    rejected = builtins.filter (assertion: !assertion.assertion && pkgs.lib.hasInfix "backup project name" assertion.message) configured.config.assertions;
  in
    rejected == [];
  boundaryName = count: pkgs.lib.concatStrings (builtins.genList (_: "a") count);
  transitionNameAccepted = name: let
    configured = evaluate [
      module
      ({config, ...}: {
        services.harbor-db.projects.${name}.transition =
          (builtins.removeAttrs config.services.harbor-db.projects.fixture.transition ["manifest"])
          // {
            barrierDirectory = "/var/lib/named-transition";
            runtimeUnits = ["named.service"];
          };
      })
    ];
  in
    builtins.filter (assertion: !assertion.assertion && pkgs.lib.hasInfix "transition project name" assertion.message) configured.config.assertions == [];
  backupPolicyAccepted = backup: let
    configured = evaluate [module {services.harbor-db.projects.fixture.backup = backup;}];
    rejected = builtins.filter (assertion: !assertion.assertion && pkgs.lib.hasPrefix "Harbor DB fixture backup " assertion.message) configured.config.assertions;
  in
    rejected == [];
  withoutPlaceholder = stage: placeholder:
    backupCommands // {${stage} = builtins.filter (arg: arg != placeholder) backupCommands.${stage};};
  transitionCommands = pkgs.lib.genAttrs ["import" "verify-target" "verify-source" "health"] (_: {
    user = "fixture";
    argv = ["${pkgs.coreutils}/bin/true"];
  });
  transitionCommandsAccepted = commands: let
    configured = evaluate [module {services.harbor-db.projects.fixture.transition.commands = pkgs.lib.mkForce commands;}];
  in
    builtins.filter (assertion: !assertion.assertion && pkgs.lib.hasPrefix "Harbor DB fixture transition" assertion.message) configured.config.assertions == [];
  backupRootsAccepted = {
    directory,
    user ? "fixture",
    enable ? true,
  }: let
    configured = evaluate [
      module
      {
        services.harbor-db.projects.peer.backup = {
          inherit directory user enable;
          group = "fixture";
          commands = backupCommands;
        };
      }
    ];
  in
    builtins.filter (assertion: !assertion.assertion && pkgs.lib.hasPrefix "Harbor DB backup roots" assertion.message) configured.config.assertions == [];
  transitionPoliciesAccepted = {
    peer ? {},
    fixture ? {},
  }: let
    configured = evaluate [
      module
      ({config, ...}: {
        services.harbor-db.projects.fixture.transition = fixture;
        services.harbor-db.projects.peer.transition =
          (builtins.removeAttrs config.services.harbor-db.projects.fixture.transition ["manifest"])
          // {
            barrierDirectory = "/var/lib/harbor-db-transitions/peer";
            runtimeUnits = ["peer.service"];
            retiredUnits = [];
          }
          // peer;
      })
    ];
  in
    builtins.filter (assertion: !assertion.assertion && pkgs.lib.hasPrefix "Harbor DB transition namespaces" assertion.message) configured.config.assertions == [];
  transitionNamespacesAccepted = peer: transitionPoliciesAccepted {inherit peer;};
  directEval = evaluate [
    ./module.nix
    ./postgres-lifecycle.nix
    ./cutover.nix
    ./pg-backup.nix
  ];
  usesPackage = evaluated: package:
    (builtins.fromJSON (builtins.unsafeDiscardStringContext evaluated.config.services.harbor-db.projects.fixture.transition.manifest.text)).storage_package
    == "${package}/bin"
    && pkgs.lib.all
    (command:
      pkgs.lib.hasInfix
      (builtins.unsafeDiscardStringContext "${package}/bin/")
      (builtins.unsafeDiscardStringContext command))
    (with evaluated.config; [
      systemd.services.harbor-db-fixture-provision.serviceConfig.ExecStart
      systemd.services.harbor-db-fixture-permissions.serviceConfig.ExecStartPost
      systemd.services.harbor-db-fixture-backup.serviceConfig.ExecStart
      systemd.services.pg-backup-prune.script
      system.preSwitchChecks."00---harbor-db-cutover"
    ]);
in
  (import ./eval-checks.nix {inherit pkgs;}).mkEvalCheck {
    name = "harbor-db-native-storage-eval";
    resultMessage = "Native storage package is selected through the module argument";
    assertions = [
      {
        name = "transition-only-resource-name-boundaries";
        message = "Transition-only projects must use the runtime's bounded resource-name grammar.";
        assertion =
          pkgs.lib.all transitionNameAccepted ["valid_Name-0" (boundaryName 128)]
          && pkgs.lib.all (name: !transitionNameAccepted name) ["" "with.dot" "with@instance" "non-ascii-é" (boundaryName 129)];
      }
      {
        name = "backup-roots-are-exclusive-normalized-namespaces";
        message = "Enabled projects must have distinct normalized backup roots, regardless of owner; disabled projects do not reserve a namespace.";
        assertion =
          backupRootsAccepted {directory = "/var/lib/peer-backup";}
          && backupRootsAccepted {
            directory = "/var/lib/fixture-backup";
            enable = false;
          }
          && !(backupRootsAccepted {directory = "/var/lib/fixture-backup";})
          && !(backupRootsAccepted {directory = "/var/lib/fixture-backup/peer";})
          && !(backupRootsAccepted {directory = "/var/lib";})
          && backupRootsAccepted {directory = "/var/lib/fixture-backup-peer";}
          && !(backupRootsAccepted {
            directory = "/var/lib/fixture-backup";
            user = "other";
          })
          && pkgs.lib.all (directory: !(backupRootsAccepted {inherit directory;}))
          ["/var/lib/./fixture-backup" "/var/lib/peer/../fixture-backup" "/var/lib//fixture-backup" "/var/lib/fixture-backup/"];
      }
      {
        name = "transition-writers-have-one-owner";
        message = "Active and retired writers must belong to only one enabled transition.";
        assertion =
          transitionNamespacesAccepted {}
          && !(transitionNamespacesAccepted {runtimeUnits = ["fixture.service"];})
          && !(transitionNamespacesAccepted {retiredUnits = ["fixture.service"];})
          && !(transitionNamespacesAccepted {runtimeUnits = ["peer.service" "peer.service"];})
          && !(transitionPoliciesAccepted {
            fixture.retiredUnits = ["legacy.service"];
            peer.runtimeUnits = ["legacy.service"];
          })
          && !(transitionPoliciesAccepted {
            fixture.retiredUnits = ["legacy.service"];
            peer.retiredUnits = ["legacy.service"];
          })
          && transitionPoliciesAccepted {fixture.retiredUnits = ["legacy.service"];}
          && transitionNamespacesAccepted {
            enable = false;
            runtimeUnits = ["fixture.service"];
          };
      }
      {
        name = "transition-barriers-have-dedicated-disjoint-namespaces";
        message = "Barrier namespaces must be normalized and non-overlapping with enabled transition or backup namespaces.";
        assertion =
          transitionNamespacesAccepted {}
          && transitionNamespacesAccepted {
            enable = false;
            barrierDirectory = "/var/lib/harbor-db-transitions/fixture";
          }
          && pkgs.lib.all (barrierDirectory: !(transitionNamespacesAccepted {inherit barrierDirectory;}))
          ["/var/lib/harbor-db-transitions/fixture" "/var/lib/harbor-db-transitions/fixture/peer" "/var/lib/harbor-db-transitions" "/var/lib/./harbor-db-transitions/peer" "/var/lib/harbor-db-transitions/other/../peer" "/var/lib//harbor-db-transitions/peer" "/var/lib/harbor-db-transitions/peer/" "/var/lib/fixture-backup" "/var/lib/fixture-backup/barrier"];
      }
      {
        name = "transition-worker-argv-boundaries";
        message = "Every declared transition worker must have nonempty argv with an absolute executable.";
        assertion =
          transitionCommandsAccepted transitionCommands
          && pkgs.lib.all (stage:
            pkgs.lib.all (argv:
              !transitionCommandsAccepted (transitionCommands // {${stage} = transitionCommands.${stage} // {inherit argv;};}))
            [[] [""] ["relative-tool"]]) (builtins.attrNames transitionCommands);
      }
      {
        name = "backup-command-placeholder-boundaries";
        message = "All stages require a literal backup argument, and restore/verify/cleanup also require a literal workspace argument.";
        assertion =
          backupPolicyAccepted {commands = pkgs.lib.mkForce backupCommands;}
          && pkgs.lib.all (stage: !backupPolicyAccepted {commands = pkgs.lib.mkForce (withoutPlaceholder stage "{backup}");}) ["capture" "restore" "verify" "cleanup"]
          && pkgs.lib.all (stage: !backupPolicyAccepted {commands = pkgs.lib.mkForce (withoutPlaceholder stage "{workspace}");}) ["restore" "verify" "cleanup"]
          && !backupPolicyAccepted {commands = pkgs.lib.mkForce (backupCommands // {capture = ["${pkgs.coreutils}/bin/true" "--backup={backup}"];});};
      }
      {
        name = "backup-credential-name-boundaries";
        message = "Credential IDs must be bounded filename-safe ASCII names before generating LoadCredential.";
        assertion =
          pkgs.lib.all (name: backupPolicyAccepted {credentials.${name} = "/run/fixture-secret";}) ["A_0-z.token" (boundaryName 255)]
          && pkgs.lib.all (name: !backupPolicyAccepted {credentials.${name} = "/run/fixture-secret";}) ["" "." ".." "a:b" "a/b" "a b" "a\n" (boundaryName 256)];
      }
      {
        name = "backup-resource-name-boundaries";
        message = "Enabled backup project names must match the runtime resource alphabet and 1..128-byte boundary.";
        assertion = backupNameAccepted "A_0-z" && backupNameAccepted (boundaryName 128) && pkgs.lib.all (name: !backupNameAccepted name) ["a.b" "a@b" "" (boundaryName 129)];
      }
      {
        name = "default-native-package";
        message = "The default flake module selects Rust without native-storage enrollment";
        assertion =
          defaultEval.config.services.harbor-db.postgresql.package
          == nativePackage
          && defaultEval.config.services.harbor-db.cutover.package == nativePackage
          && usesPackage defaultEval nativePackage;
      }
      {
        name = "direct-module-native-package";
        message = "Direct module imports also select Rust for every lifecycle adapter";
        assertion =
          (directEval.config.services.harbor-db.postgresql.package.harborDbRuntime or null)
          == "rust"
          && directEval.config.services.harbor-db.postgresql.package == directEval.config.services.harbor-db.cutover.package
          && usesPackage directEval directEval.config.services.harbor-db.postgresql.package;
      }
      {
        name = "postgres-native-package";
        message = "PostgreSQL lifecycle selects the native package";
        assertion = eval.config.services.harbor-db.postgresql.package == nativePackage;
      }
      {
        name = "cutover-native-package";
        message = "Cutover dispatch selects the native package";
        assertion = eval.config.services.harbor-db.cutover.package == nativePackage;
      }
      {
        name = "postgres-native-prestart";
        message = "The managed PostgreSQL pre-start hook executes the native adapter";
        assertion =
          pkgs.lib.hasInfix
          (builtins.unsafeDiscardStringContext "${nativePackage}/bin/harbor-db-postgres")
          (builtins.unsafeDiscardStringContext eval.config.systemd.services.postgresql.preStart);
      }
    ];
  }
