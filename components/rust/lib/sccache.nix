# mkSccacheEnv       :: { bucket, endpoint, region?, keyPrefix?, useSsl?, accessKeyId?, secretAccessKey? }
#                     -> { SCCACHE_*, AWS_* }
#
# mkSccacheCraneEnv  :: { enable?, package?, bucket?, endpoint?, region?, keyPrefix?, useSsl?,
#                         accessKeyId?, secretAccessKey?, connectTimeout?, daemonUds? }
#                     -> { RUSTC_WRAPPER?, SCCACHE_*, AWS_* }
#
# wrapRustPackageWithSccache :: { package, sccachePackage, envVars, enable? } -> package
#
# Generate sccache environment variables for dev shells (mkSccacheEnv) or
# for injection into crane `commonArgs` for derivation-level sccache (mkSccacheCraneEnv).
# When `enable = false` or required S3 params are missing, mkSccacheCraneEnv returns {}.
# When `daemonUds` is set (e.g. "/run/sccache/sock"), emits SCCACHE_SERVER_UDS and
# omits SCCACHE_DIR/XDG_CACHE_HOME — the daemon owns disk caching.  Callers on hosts
# with a persistent sccache daemon (managed by the NixOS module in daemon mode) should
# pass daemonUds; otherwise the helper defaults to per-build local-only cache.
_: let
  inherit (builtins) isString stringLength;

  # Build the base SCCACHE_* + AWS_* env var set shared by both functions.
  mkSccacheInner = {
    bucket,
    endpoint,
    region ? "auto",
    keyPrefix ? null,
    useSsl ? false,
    accessKeyId ? null,
    secretAccessKey ? null,
  }:
    assert isString bucket
    || throw "harbor-rs: mkSccacheEnv 'bucket' is required and must be a string";
    assert stringLength bucket
    > 0
    || throw "harbor-rs: mkSccacheEnv 'bucket' must not be empty";
    assert isString endpoint
    || throw "harbor-rs: mkSccacheEnv 'endpoint' is required and must be a string";
    assert stringLength endpoint
    > 0
    || throw "harbor-rs: mkSccacheEnv 'endpoint' must not be empty";
      {
        SCCACHE_BUCKET = bucket;
        SCCACHE_ENDPOINT = endpoint;
        SCCACHE_REGION = region;
        SCCACHE_S3_USE_SSL =
          if useSsl
          then "true"
          else "false";
      }
      // (
        if keyPrefix != null
        then {SCCACHE_S3_KEY_PREFIX = keyPrefix;}
        else {}
      )
      // (
        if accessKeyId != null
        then {AWS_ACCESS_KEY_ID = accessKeyId;}
        else {}
      )
      // (
        if secretAccessKey != null
        then {AWS_SECRET_ACCESS_KEY = secretAccessKey;}
        else {}
      );
in {
  inherit mkSccacheInner;

  mkSccacheEnv = mkSccacheInner;

  wrapRustPackageWithSccache = {
    package,
    sccachePackage,
    envVars,
    enable ? true,
  }:
    if !enable
    then package
    else
      package.overrideAttrs (old:
        {
          nativeBuildInputs = (old.nativeBuildInputs or []) ++ [sccachePackage];
        }
        // (
          if old ? env
          then {env = old.env // envVars;}
          else envVars
        ));

  mkSccacheCraneEnv = {
    enable ? true,
    package ? null,
    bucket ? null,
    endpoint ? null,
    region ? "auto",
    keyPrefix ? null,
    useSsl ? false,
    accessKeyId ? null,
    secretAccessKey ? null,
    connectTimeout ? "2",
    daemonUds ? null,
  }:
    if !enable
    then {}
    else let
      # Daemon mode — caller has a persistent sccache daemon (Unix socket
      # reachable from the sandbox).  The daemon owns all disk caching so
      # we omit SCCACHE_DIR/XDG_CACHE_HOME entirely.
      daemonEnv =
        if daemonUds != null
        then {
          RUSTC_WRAPPER = package;
          SCCACHE_SERVER_UDS = daemonUds;
          SCCACHE_CONNECT_TIMEOUT = connectTimeout;
        }
        else {};
      # Local-only mode — per-build writable tmpfs path inside the sandbox
      # so sccache has a writable HOME-adjacent directory even when the
      # NixOS module's impure-env SCCACHE_DIR is unavailable.
      localEnv = {
        RUSTC_WRAPPER = package;
        SCCACHE_DIR = "$NIX_BUILD_TOP/.sccache";
        XDG_CACHE_HOME = "$NIX_BUILD_TOP/.sccache";
        SCCACHE_CONNECT_TIMEOUT = connectTimeout;
      };
      s3Cfg =
        if bucket != null && endpoint != null
        then
          mkSccacheInner {
            inherit bucket endpoint region keyPrefix useSsl accessKeyId secretAccessKey;
          }
        else {};
      env =
        if daemonUds != null
        then daemonEnv
        else localEnv;
    in
      env // s3Cfg;
}
