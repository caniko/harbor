{
  config,
  lib,
  pkgs,
  harborDbStoragePackage,
  ...
}: let
  inherit (lib) mkOption types;
  projects = lib.filterAttrs (_: p: p.transition.enable) config.services.harbor-db.projects;
  writers = lib.concatMap (p: p.transition.runtimeUnits ++ p.transition.retiredUnits) (lib.attrValues projects);
  barriers = map (p: p.transition.barrierDirectory) (lib.attrValues projects);
  backupRoots = map (p: p.backup.directory) (lib.attrValues (lib.filterAttrs (_: p: p.backup.enable or false) config.services.harbor-db.projects));
  storagePaths = import ./storage-paths.nix {inherit lib;};
  immutableFiles = import ./immutable-files.nix {inherit lib;};
  package = harborDbStoragePackage;
  manifest = name: p: let
    document = pkgs.writeText "harbor-db-${name}-transition.json" (builtins.toJSON {
      version = 1;
      resource = name;
      source_manifest = p.sourceManifest;
      target_manifest = p.targetManifest;
      backup_manifest = p.backupManifest;
      independent_receipt = p.independentReceipt;
      custody_manifest = p.targetCustodyManifest;
      postgres_manifest = p.postgresManifest;
      postgres_socket = p.postgresSocket;
      postgres_port = p.postgresPort;
      barrier_dir = p.barrierDirectory;
      drop_in_root = "/etc/systemd/system.control";
      systemctl = "${pkgs.systemd}/bin/systemctl";
      busctl = "${pkgs.systemd}/bin/busctl";
      runuser = "${pkgs.util-linux}/bin/runuser";
      storage_package = "${package}/bin";
      units = p.runtimeUnits;
      retired_units = p.retiredUnits;
      inherit (p) commands;
      executable_files = p.executableFiles;
      timeout_seconds = p.timeoutSeconds;
    });
  in
    immutableFiles.checkedManifest "Harbor DB ${name} transition" p.executableFiles document;
in {
  imports = [./storage-package-argument.nix];
  options.services.harbor-db.projects = mkOption {
    type = types.attrsOf (types.submodule ({
      name,
      config,
      ...
    }: {
      options.transition = {
        enable = lib.mkEnableOption "explicit application backend transition tooling";
        sourceManifest = mkOption {type = types.strMatching "/nix/store/.*";};
        targetManifest = mkOption {type = types.strMatching "/nix/store/.*";};
        backupManifest = mkOption {type = types.strMatching "/nix/store/.*";};
        targetCustodyManifest = mkOption {
          type = types.nullOr (types.strMatching "/nix/store/.*");
          default = null;
          description = "Target cutover resource entry with transition_manifest omitted, for durable corpus custody publication.";
        };
        postgresManifest = mkOption {
          type = types.nullOr (types.strMatching "/nix/store/.*");
          default = null;
        };
        postgresSocket = mkOption {
          type = types.strMatching "/[A-Za-z0-9_./-]+";
          default = "/run/postgresql";
        };
        postgresPort = mkOption {
          type = types.port;
          default = 5432;
        };
        independentReceipt = mkOption {type = types.strMatching "/[A-Za-z0-9_./-]+";};
        barrierDirectory = mkOption {
          type = types.strMatching "/[A-Za-z0-9_./-]+";
          default = "/var/lib/harbor-db-transitions/${name}";
        };
        runtimeUnits = mkOption {type = types.listOf (types.strMatching "[A-Za-z0-9_@.:-]+\\.service");};
        retiredUnits = mkOption {
          type = types.listOf (types.strMatching "[A-Za-z0-9_@.:-]+\\.service");
          default = [];
        };
        commands = mkOption {
          type = types.attrsOf (types.submodule {
            options = {
              user = mkOption {type = types.strMatching "[a-z_][a-z0-9_-]*";};
              argv = mkOption {type = types.listOf types.str;};
            };
          });
          description = "Declared import, verify-target, verify-source and health workers. Imports must be resumable for the same source snapshot.";
        };
        executableFiles = mkOption {
          type = types.listOf types.str;
          default = [];
        };
        timeoutSeconds = mkOption {
          type = types.ints.between 1 86400;
          default = 1800;
        };
        manifest = mkOption {
          type = types.path;
          readOnly = true;
          default = manifest name config.transition;
        };
      };
    }));
  };
  config = lib.mkIf (projects != {}) {
    assertions =
      [
        {
          assertion = lib.length writers == lib.length (lib.unique writers);
          message = "Harbor DB transition namespaces require each active or retired writer unit to have one enabled transition owner.";
        }
        {
          assertion = lib.all (path: path != "/" && storagePaths.normalizedAbsolute path) barriers;
          message = "Harbor DB transition namespaces require dedicated normalized absolute barrier directories.";
        }
        {
          assertion = storagePaths.disjoint (barriers ++ backupRoots);
          message = "Harbor DB transition namespaces require barrier and enabled backup roots to be pairwise non-overlapping.";
        }
      ]
      ++ lib.concatLists (lib.mapAttrsToList (name: p: [
          {
            assertion = builtins.match "[A-Za-z0-9_-]{1,128}" name != null;
            message = "Harbor DB ${name} transition project name must contain 1..128 ASCII letters, digits, underscores or hyphens, matching the runtime resource name.";
          }
          {
            assertion = builtins.attrNames p.transition.commands == ["health" "import" "verify-source" "verify-target"] && p.transition.runtimeUnits != [];
            message = "Harbor DB ${name} transitions require all semantic workers and explicit runtime units.";
          }
          {
            assertion = lib.all (worker: worker.argv != [] && lib.hasPrefix "/" (lib.head worker.argv)) (lib.attrValues p.transition.commands);
            message = "Harbor DB ${name} transition workers require nonempty argv with an absolute executable.";
          }
          {
            assertion = lib.all immutableFiles.storePath p.transition.executableFiles;
            message = "Harbor DB ${name} transition executableFiles must be normalized immutable store paths; manifest realization requires regular files.";
          }
        ])
        projects);
    systemd.tmpfiles.rules = lib.concatMap (p: [
      "d ${p.transition.barrierDirectory} 0700 root root -"
      "f ${p.transition.barrierDirectory}/lock 0600 root root -"
    ]) (lib.attrValues projects);
    environment.etc = lib.mapAttrs' (name: p: lib.nameValuePair "harbor-db/${name}-transition.json" {source = manifest name p.transition;}) projects;
    environment.systemPackages = [package];
  };
}
