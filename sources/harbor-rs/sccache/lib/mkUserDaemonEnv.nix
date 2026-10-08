{
  lib,
  sccacheDefault,
}: {
  remoteEnv,
  extraEnv,
  basedirs,
  idleTimeout,
  multiLevelChain,
  redisEndpoint,
  redisKeyPrefix,
  redisRwMode,
  multiLevelWriteErrorPolicy,
  cacheHome,
}:
remoteEnv
// extraEnv
// lib.optionalAttrs (basedirs != []) {
  SCCACHE_BASEDIRS = lib.concatStringsSep ":" basedirs;
}
// lib.optionalAttrs (multiLevelChain != null) {
  SCCACHE_MULTILEVEL_CHAIN = multiLevelChain;
}
// lib.optionalAttrs (redisEndpoint != null) {
  SCCACHE_REDIS_ENDPOINT = redisEndpoint;
}
// lib.optionalAttrs (redisKeyPrefix != null) {
  SCCACHE_REDIS_KEY_PREFIX = redisKeyPrefix;
}
// lib.optionalAttrs (redisRwMode != null) {
  SCCACHE_REDIS_RW_MODE = redisRwMode;
}
// lib.optionalAttrs (multiLevelWriteErrorPolicy != null) {
  SCCACHE_MULTILEVEL_WRITE_ERROR_POLICY = multiLevelWriteErrorPolicy;
}
// {
  SCCACHE_SERVER_UDS = "%t/${sccacheDefault.userSocketRel}";
  SCCACHE_DIR = "${cacheHome}/${sccacheDefault.userCacheRel}";
  SCCACHE_IDLE_TIMEOUT = toString idleTimeout;
  SCCACHE_START_SERVER = "1";
  SCCACHE_NO_DAEMON = "1";
  SCCACHE_LOG = "warn";
}
