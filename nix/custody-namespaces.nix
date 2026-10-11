{
  config,
  lib,
  options,
}: let
  enabled = options.services.harbor-db ? cutover && config.services.harbor-db.cutover.enable;
  postgresEnabled = options.services.harbor-db ? postgresql && config.services.harbor-db.postgresql.enable;
  filesystem = lib.concatMap (entry: entry.authority.directories ++ [entry.authority.state_dir]) (lib.attrValues config.services.harbor-db.cutover.resources);
in
  lib.optionals enabled (filesystem
    ++ lib.optionals postgresEnabled [config.services.postgresql.dataDir config.services.harbor-db.postgresql.stateDir config.services.harbor-db.postgresql.writerFence.startupStateDir])
