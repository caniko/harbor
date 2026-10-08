{
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.services.harborRs.mavenCache;
  allowedExtensions = [
    ".jar"
    ".war"
    ".pom"
    ".xml"
    ".module"
    ".md5"
    ".sha1"
    ".sha256"
    ".sha512"
    ".asc"
    ".aar"
  ];
  mirror = reference: {
    inherit reference allowedExtensions;
    store = true;
    allowedGroups = [];
    connectTimeout = 3;
    readTimeout = 15;
    httpProxy = "";
    authenticatedFetchingOnly = false;
  };
  sharedConfiguration = (pkgs.formats.json {}).generate "reposilite-shared.json" {
    maven.repositories = [
      {
        id = "cache";
        visibility = "PUBLIC";
        redeployment = false;
        preserveSnapshots = false;
        storageProvider = {
          type = "fs";
          quota = cfg.storageQuota;
          mount = cfg.storageMount;
          maxResourceLockLifetimeInSeconds = 60;
        };
        storagePolicy = "PRIORITIZE_UPSTREAM_METADATA";
        metadataMaxAge = 0;
        proxied = map mirror cfg.mirrors;
      }
    ];
  };
in {
  options.services.harborRs.mavenCache = {
    enable = lib.mkEnableOption "Harbor Maven proxy cache";

    mirrors = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [
        "https://repo.maven.apache.org/maven2/"
        "https://dl.google.com/dl/android/maven2/"
        "https://plugins.gradle.org/m2/"
      ];
      description = "Ordered Maven repositories proxied by Reposilite.";
    };

    storageQuota = lib.mkOption {
      type = lib.types.str;
      default = "64GB";
      description = "Reposilite filesystem repository quota.";
    };

    storageMount = lib.mkOption {
      type = lib.types.str;
      default = "";
      description = "Optional filesystem path used to store cached artifacts.";
    };

    sharedConfiguration = lib.mkOption {
      type = lib.types.path;
      readOnly = true;
      internal = true;
      description = "Generated Reposilite shared configuration.";
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = config.services.reposilite.database.path != "--temporary";
        message = "Harbor Maven cache requires a persistent Reposilite database";
      }
      {
        assertion = builtins.match "^[0-9]+(KB|MB|GB|%)?$" cfg.storageQuota != null;
        message = "Harbor Maven cache quota must use bytes, KB, MB, GB, or percent";
      }
      {
        assertion = cfg.storageMount == "" || lib.hasPrefix "/" cfg.storageMount;
        message = "Harbor Maven cache storageMount must be empty or absolute";
      }
    ];

    services.harborRs.mavenCache.sharedConfiguration = sharedConfiguration;

    services.reposilite = {
      enable = true;
      database = {
        type = lib.mkDefault "sqlite";
        path = lib.mkDefault "reposilite.db";
      };
      settings.hostname = lib.mkDefault "127.0.0.1";
      extraArgs = lib.mkAfter [
        "--shared-configuration"
        (toString sharedConfiguration)
      ];
    };
  };
}
