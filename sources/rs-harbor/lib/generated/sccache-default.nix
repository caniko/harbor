{
  systemSocketPath = "/run/sccache/sock";
  systemDiskCacheDir = "/var/lib/sccache";
  userSocketRel = "sccache/sock";
  userCacheRel = "sccache";
  connectTimeout = "2";
  cacheRegion = "auto";
  cacheUseSsl = false;
  idleTimeout = 0;
  startupTimeout = 20;
  restartDelay = 5;
  restartBurst = 3;
  restartWindow = 300;
}
