# Generic derivation-level compiler-cache policy.
#
# Host modules own cache transport, credentials, and the writable sandbox
# mount.  This library owns the mechanics that must be identical in every
# Rust consumer: the versioned namespace, sandbox admission, environment
# scrubbing, Cargo/Dioxus wrapper selection, and Cargo artifact propagation.
{}: let
  buildContract = import ./build-contract.nix {};
  remoteCacheEnvNames = [
    "SCCACHE_SERVER_UDS"
    "SCCACHE_BUCKET"
    "SCCACHE_ENDPOINT"
    "SCCACHE_REGION"
    "SCCACHE_S3_USE_SSL"
    "SCCACHE_S3_KEY_PREFIX"
    "AWS_ACCESS_KEY_ID"
    "AWS_SECRET_ACCESS_KEY"
    "AWS_SESSION_TOKEN"
  ];

  # These variables are safe to pass into a Nix sandbox when the cache
  # transport is a host-mounted Unix socket.  Credentials are deliberately
  # absent from this list and remain scrubbed from derivations.
  sharedTransportEnvNames = [
    "SCCACHE_REDIS_ENDPOINT"
    "SCCACHE_REDIS_CLUSTER_ENDPOINTS"
    "SCCACHE_REDIS_DB"
    "SCCACHE_REDIS_KEY_PREFIX"
    "SCCACHE_REDIS_RW_MODE"
    "SCCACHE_MULTILEVEL_CHAIN"
    "SCCACHE_MULTILEVEL_WRITE_ERROR_POLICY"
    "SCCACHE_BASEDIRS"
  ];

  mkBuildCachePolicy = {
    pkgs,
    # Cross consumers may pass the build package set explicitly.  The cache
    # executable and its wrapper always run on the build platform, never on
    # the target platform.
    buildPackageSet ? null,
    sccachePackage ? null,
    cacheRoot ? null,
    namespaceScope ? "rs-harbor-rust",
    namespaceGeneration ? 1,
    connectTimeout ? "2",
    executionModel ? "sandbox-local",
    # Atlas mounts this socket into Nix sandboxes.  The wrapper discovers it
    # at runtime so every consumer using rs-harbor gets the host transport
    # without copying host-specific Redis settings into project flakes.
    redisSocketPath ? "/run/redis-sccache/redis.sock",
    compiler ? null,
    toolchainManifest ? null,
  }: let
    lib = pkgs.lib;
    buildPackages =
      if buildPackageSet != null
      then buildPackageSet
      else pkgs.buildPackages or pkgs;
    canonicalSccachePackage =
      if sccachePackage != null
      then sccachePackage
      else buildPackages.sccache;
    sccacheVersion = canonicalSccachePackage.version or "unknown";
    namespace = "${namespaceScope}-v${toString namespaceGeneration}-sccache-${sccacheVersion}";
    sharedCacheDir =
      if cacheRoot == null
      then null
      else "${cacheRoot}/${namespace}";
    compilerName =
      if compiler != null
      then compiler
      else if buildPackages ? rustc
      then buildPackages.rustc.version
      else "rustc";
    contractToolchain =
      if toolchainManifest != null
      then toolchainManifest
      else buildContract.toolchain.toolchain;

    mkSandboxWrapper = packageSet: let
      realSccache =
        if packageSet ? sccache
        then packageSet.sccache
        else canonicalSccachePackage;
      wrapperCacheDir =
        if sharedCacheDir == null
        then ""
        else sharedCacheDir;
    in
      packageSet.writeShellScriptBin "rs-harbor-sandbox-sccache" ''
        set -eu

        state_root="''${NIX_BUILD_TOP:-''${TMPDIR:-/tmp}}"
        compiler_socket="$state_root/rs-harbor-sccache-server.sock"
        configured_cache_dir=${lib.escapeShellArg wrapperCacheDir}

        shared_cache_is_admissible() {
          current_uid="$(${packageSet.coreutils}/bin/id -u)"
          current_gid="$(${packageSet.coreutils}/bin/id -g)"
          current_user="$(${packageSet.coreutils}/bin/id -un)"
          current_group="$(${packageSet.coreutils}/bin/id -gn)"

          case "$current_user:$current_group" in
            nixbld*:nixbld) ;;
            *) return 1 ;;
          esac

          ${packageSet.coreutils}/bin/mkdir -p "$configured_cache_dir" 2>/dev/null || return 1
          [ -d "$configured_cache_dir" ] \
            && [ ! -L "$configured_cache_dir" ] \
            && [ -w "$configured_cache_dir" ] \
            || return 1

          owner_uid="$(${packageSet.coreutils}/bin/stat -c %u "$configured_cache_dir")"
          owner_gid="$(${packageSet.coreutils}/bin/stat -c %g "$configured_cache_dir")"
          owner_name="$(${packageSet.coreutils}/bin/stat -c %U "$configured_cache_dir")"
          group_name="$(${packageSet.coreutils}/bin/stat -c %G "$configured_cache_dir")"
          mode="$(${packageSet.coreutils}/bin/stat -c %a "$configured_cache_dir")"

          case "$owner_name:$owner_uid" in
            root:0 | nixbld*:* | nobody:65534) ;;
            *) return 1 ;;
          esac
          [ "$owner_gid" = "$current_gid" ] \
            && [ "$group_name" = "$current_group" ] \
            || return 1
          case "$mode" in
            "" | *[!0-7]*) return 1 ;;
          esac
          [ "$((8#$mode & 0777))" -eq "$((0770))" ]
        }

        # Sandbox builds must never send Garage credentials or a host compiler
        # daemon socket through the compiler wrapper.  Redis is different:
        # it stores opaque compiler artifacts and does not execute rustc, so
        # it is safe to expose through a host-mounted Unix socket.
        unset ${lib.escapeShellArgs remoteCacheEnvNames}
        unset SCCACHE_SERVER_UDS

        # Nix's impure-env setting is not inherited by ordinary derivations;
        # it only affects fixed-output builders.  Discover the canonical
        # host-mounted Redis socket here instead of silently falling back to
        # a private $NIX_BUILD_TOP cache when a project uses cacheRoot=null.
        if [ -z "''${SCCACHE_REDIS_ENDPOINT:-}" ] \
          && [ -z "''${SCCACHE_REDIS_CLUSTER_ENDPOINTS:-}" ] \
          && [ -S ${lib.escapeShellArg redisSocketPath} ]; then
          # redis-rs 0.32 accepts the Unix socket when the URI has an
          # authority component; the authority is ignored for the socket
          # path but avoids its `invalid format` rejection of an empty one.
          export SCCACHE_REDIS_ENDPOINT=${lib.escapeShellArg "redis+unix://localhost${redisSocketPath}"}
        fi

        if [ -n "''${SCCACHE_REDIS_ENDPOINT:-}" ] || [ -n "''${SCCACHE_REDIS_CLUSTER_ENDPOINTS:-}" ]; then
          if [ -z "''${SCCACHE_REDIS_KEY_PREFIX:-}" ]; then
            export SCCACHE_REDIS_KEY_PREFIX=${lib.escapeShellArg "canix/${namespace}"}
          fi
          if [ -z "''${SCCACHE_REDIS_RW_MODE:-}" ]; then
            export SCCACHE_REDIS_RW_MODE=READ_WRITE
          fi
          if [ -z "''${SCCACHE_MULTILEVEL_CHAIN:-}" ]; then
            export SCCACHE_MULTILEVEL_CHAIN=redis
          fi
          if [ -z "''${SCCACHE_MULTILEVEL_WRITE_ERROR_POLICY:-}" ]; then
            export SCCACHE_MULTILEVEL_WRITE_ERROR_POLICY=ignore
          fi
          if [ -z "''${SCCACHE_BASEDIRS:-}" ]; then
            export SCCACHE_BASEDIRS=/build
          fi
          unset SCCACHE_DIR XDG_CACHE_HOME
          export SCCACHE_SERVER_UDS="$compiler_socket"
        else
          unset XDG_CACHE_HOME

          umask 0007
          if [ -n "$configured_cache_dir" ]; then
            if ! shared_cache_is_admissible; then
              echo "rs-harbor sccache: configured sandbox cache is not admissible; refusing an uncached build" >&2
              exit 75
            fi
            cache_dir="$configured_cache_dir"
          elif [ -n "''${SCCACHE_DIR:-}" ] && [ -d "''${SCCACHE_DIR}" ] && [ -w "''${SCCACHE_DIR}" ]; then
            cache_dir="$SCCACHE_DIR"
          else
            echo "rs-harbor sccache: no managed cache transport is available; refusing an uncached build" >&2
            exit 75
          fi

          export SCCACHE_DIR="$cache_dir"
          export XDG_CACHE_HOME="$cache_dir"
        fi

        export SCCACHE_SERVER_UDS="$compiler_socket"
        export SCCACHE_CONNECT_TIMEOUT=${lib.escapeShellArg connectTimeout}

        # Start exactly one server per sandbox.  The launcher is bounded by
        # the atomic lock and the socket check; a failed backend check is a
        # hard build failure instead of an implicit uncached compiler run.
        ready="$state_root/rs-harbor-sccache-ready"
        startup_lock="$state_root/rs-harbor-sccache-startup.lock"
        startup_log="$state_root/rs-harbor-sccache-startup.log"
        attempts=0
        while [ ! -d "$ready" ] && [ "$attempts" -lt 200 ]; do
          if ${packageSet.coreutils}/bin/mkdir "$startup_lock" 2>/dev/null; then
            cleanup_startup_lock() {
              ${packageSet.coreutils}/bin/rmdir "$startup_lock" 2>/dev/null || true
            }
            trap cleanup_startup_lock EXIT
            trap 'cleanup_startup_lock; exit 1' HUP INT TERM

            previous_umask=$(umask)
            umask 0007
            if ${realSccache}/bin/sccache --start-server >"$startup_log" 2>&1 \
              && [ -S "$compiler_socket" ]; then
              ${packageSet.coreutils}/bin/mkdir -p "$ready"
            else
              ${packageSet.coreutils}/bin/cat "$startup_log" >&2 || true
              echo "rs-harbor sccache: managed server failed to become ready; refusing an uncached build" >&2
              cleanup_startup_lock
              trap - EXIT HUP INT TERM
              exit 75
            fi
            umask "$previous_umask"

            cleanup_startup_lock
            trap - EXIT HUP INT TERM
            break
          fi

          ${packageSet.coreutils}/bin/sleep 0.01
          attempts=$((attempts + 1))
        done

        if [ ! -d "$ready" ] || [ ! -S "$compiler_socket" ]; then
          echo "rs-harbor sccache: timed out waiting for the managed server; refusing an uncached build" >&2
          exit 75
        fi

        exec ${realSccache}/bin/sccache "$@"
      '';

    markSandboxWrapper = wrapperDrv:
      wrapperDrv.overrideAttrs (old: {
        passthru =
          (old.passthru or {})
          // {
            rsHarborSandboxLocalSccache = true;
            rsHarborSharedCacheDir = sharedCacheDir;
            sharedCacheAdmission = "nixbld-owner-group-0770";
          };
      });
    wrapper = markSandboxWrapper (mkSandboxWrapper buildPackages);
    wrapperPath = "${wrapper}/bin/rs-harbor-sandbox-sccache";
    dioxusDispatcher = buildPackages.writeShellScriptBin "rs-harbor-dioxus-sccache" ''
      set -eu

      # Cargo invokes RUSTC_WRAPPER as:
      #   wrapper rustc [args...]
      # or, for Dioxus workspace crates:
      #   wrapper dx rustc [args...]
      # Dioxus must remain the workspace wrapper so it can capture asset and
      # linker arguments.  Dependencies, however, should go through
      # sccache.  A patched Dioxus CLI also consumes DX_RUSTC_INNER_WRAPPER
      # for its direct rustc replay path.
      compiler="''${1:-}"
      case "$compiler" in
        */dx|dx|*/.dx-wrapped|.dx-wrapped)
          exec "$@"
          ;;
        *)
          exec ${wrapperPath} "$@"
          ;;
      esac
    '';
    dioxusDispatcherPath = "${dioxusDispatcher}/bin/rs-harbor-dioxus-sccache";
    telemetryHook = buildPackages.writeTextFile {
      name = "rs-harbor-sccache-telemetry-hook";
      destination = "/nix-support/setup-hook";
      text = ''
        if [ -z "''${RS_HARBOR_SCCACHE_TELEMETRY_LOADED:-}" ]; then
          export RS_HARBOR_SCCACHE_TELEMETRY_LOADED=1

          rs_harbor_sccache_telemetry_start() {
            if [ "''${RS_HARBOR_SCCACHE_TELEMETRY_STARTED:-0}" = 1 ]; then
              return 0
            fi
            ${wrapperPath} --zero-stats >/dev/null 2>&1 || return 0
            export RS_HARBOR_SCCACHE_TELEMETRY_STARTED=1
            export RS_HARBOR_SCCACHE_TELEMETRY_STARTED_AT="$(${buildPackages.coreutils}/bin/date +%s)"
          }

          rs_harbor_sccache_telemetry_emit() {
            outcome="$1"
            exit_code="''${2:-0}"
            [ "''${RS_HARBOR_SCCACHE_TELEMETRY_STARTED:-0}" = 1 ] || return 0
            stats="$(${wrapperPath} --show-adv-stats --stats-format json 2>/dev/null || printf '{}')"
            ${buildPackages.jq}/bin/jq -c \
              --arg outcome "$outcome" \
              --arg exitCode "$exit_code" \
              --arg name "''${name:-unknown}" \
              --arg workloadKind "''${RS_HARBOR_SCCACHE_WORKLOAD_KIND:-unknown}" \
              --arg reuseKey "''${RS_HARBOR_SCCACHE_REUSE_KEY:-}" \
              --arg compiler "''${RS_HARBOR_SCCACHE_COMPILER:-''${RUSTC:-rustc}}" \
              --arg targetTriple "''${RS_HARBOR_SCCACHE_TARGET_TRIPLE:-''${CARGO_BUILD_TARGET:-''${TARGET:-unknown}}}" \
              --arg startedAt "''${RS_HARBOR_SCCACHE_TELEMETRY_STARTED_AT:-}" \
              --arg emittedAt "$(${buildPackages.coreutils}/bin/date +%s)" \
              --arg namespace ${lib.escapeShellArg namespace} \
              --arg sccacheVersion ${lib.escapeShellArg sccacheVersion} \
              'if (.stats | type) != "object" then empty else . + {
                rsHarborTelemetrySchemaVersion: 1,
                rsHarborOutcome: $outcome,
                rsHarborExitCode: ($exitCode | tonumber),
                rsHarborDerivation: $name,
                rsHarborWorkloadKind: $workloadKind,
                rsHarborReuseKey: $reuseKey,
                compiler: $compiler,
                targetTriple: $targetTriple,
                rsHarborStartedAt: ($startedAt | tonumber),
                rsHarborEmittedAt: ($emittedAt | tonumber),
                rsHarborNamespace: $namespace,
                rsHarborSccacheVersion: $sccacheVersion
              } end' <<<"$stats" \
              | sed 's/^/RS_HARBOR_SCCACHE_STATS_V1 /'
            ${wrapperPath} --stop-server >/dev/null 2>&1 || true
          }
          rs_harbor_sccache_telemetry_success() {
            rs_harbor_sccache_telemetry_emit success "''${exitCode:-0}"
          }
          rs_harbor_sccache_telemetry_failure() {
            rs_harbor_sccache_telemetry_emit failure "''${exitCode:-1}"
          }

          preConfigureHooks+=(rs_harbor_sccache_telemetry_start)
          preBuildHooks+=(rs_harbor_sccache_telemetry_start)
          exitHooks+=(rs_harbor_sccache_telemetry_success)
          failureHooks+=(rs_harbor_sccache_telemetry_failure)
        fi
      '';
    };
    commonNativeInputs = [wrapper telemetryHook buildPackages.jq];

    rustEnv = {
      RUSTC_WRAPPER = wrapperPath;
      CARGO_INCREMENTAL = "0";
    };
    dioxusEnv = {
      # Dioxus installs its own compiler shim in RUSTC_WORKSPACE_WRAPPER.
      # The dispatcher caches ordinary dependency crates while preserving
      # Dioxus' workspace argument capture.  Newer patched dx binaries also
      # use the inner wrapper for their direct rustc replay path.
      RUSTC_WRAPPER = dioxusDispatcherPath;
      DX_RUSTC_INNER_WRAPPER = wrapperPath;
      CARGO_INCREMENTAL = "0";
    };

    markWrapped = old: {
      passthru =
        (old.passthru or {})
        // {
          rsHarborBuildCacheWrapped = true;
          rsHarborBuildCacheContract = {
            inherit namespace sccacheVersion executionModel;
          };
        };
    };

    withEnv = {
      package,
      env,
      directEnvOverrides ? {},
      nativeInputs ? commonNativeInputs,
      markCacheWrapped ? true,
    }: let
      # Crane callers commonly pass cache variables as direct derivation
      # arguments. Nix rejects repeating those names inside `env`, so
      # preserve the direct value and only inject names that are not
      # already present on the package.
      directEnvNames = builtins.filter (name: builtins.hasAttr name package) (builtins.attrNames env);
      effectiveEnv = builtins.removeAttrs env (directEnvNames ++ builtins.attrNames directEnvOverrides);
    in
      package.overrideAttrs (old:
        if ((old.passthru or {}).rsHarborBuildCacheWrapped or false) && directEnvOverrides == {}
        then {
          # Crane can wrap mkCargoDerivation before buildDepsOnly or
          # buildPackage applies its phase-specific telemetry context.
          # Preserve the existing native inputs while allowing the outer
          # boundary to update workload metadata.
          env = (old.env or {}) // effectiveEnv;
        }
        else
          {
            nativeBuildInputs =
              if (old.passthru or {}).rsHarborBuildCacheWrapped or false
              then old.nativeBuildInputs or []
              else (old.nativeBuildInputs or []) ++ nativeInputs;
            env = (builtins.removeAttrs (old.env or {}) (builtins.attrNames directEnvOverrides)) // effectiveEnv;
          }
          // directEnvOverrides
          // lib.optionalAttrs markCacheWrapped (markWrapped old));

    withRustCache = {
      package,
      enable ? true,
      extraEnv ? {},
      linkerPackageSet ? null,
      telemetry ? {},
    }:
      if !enable
      then package
      else let
        linkerEnv =
          if linkerPackageSet == null
          then {}
          else hostLinkerEnvFor linkerPackageSet;
        targetTriple =
          telemetry.targetTriple
          or package.CARGO_BUILD_TARGET
          or ((package.env or {}).CARGO_BUILD_TARGET or buildPackages.stdenv.buildPlatform.rust.cargoEnvVarTarget);
        compilerValue = telemetry.compiler or compilerName;
        telemetryEnv = {
          RS_HARBOR_SCCACHE_WORKLOAD_KIND = telemetry.workloadKind or "unknown";
          RS_HARBOR_SCCACHE_REUSE_KEY = telemetry.reuseKey or "";
          RS_HARBOR_SCCACHE_COMPILER = compilerValue;
          RS_HARBOR_SCCACHE_TARGET_TRIPLE = targetTriple;
        };
        wrapped = withEnv {
          inherit package;
          env = rustEnv // extraEnv // linkerEnv // telemetryEnv;
          # mkCargoDerivation installs placeholder telemetry before Crane's
          # phase-specific builders run.  Override those direct derivation
          # attributes here so buildDepsOnly/buildPackage do not retain the
          # inner "unknown"/empty values.
          directEnvOverrides = linkerEnv // telemetryEnv;
        };
      in
        wrapped.overrideAttrs (old:
          if ((old.passthru or {}).rsHarborBuildCacheArtifactsWrapped or false) && linkerEnv == {}
          then {}
          else {
            cargoArtifacts =
              if (old.cargoArtifacts or null) != null
              then
                withEnv {
                  package = old.cargoArtifacts;
                  env = rustEnv // extraEnv // linkerEnv // telemetryEnv;
                  directEnvOverrides = linkerEnv // telemetryEnv;
                }
              else old.cargoArtifacts or null;
            passthru =
              (old.passthru or {})
              // {rsHarborBuildCacheArtifactsWrapped = true;};
          });

    withDioxusCache = {
      package,
      enable ? true,
    }:
      if !enable
      then package
      else
        withEnv {
          inherit package;
          env = dioxusEnv;
          nativeInputs = commonNativeInputs ++ [dioxusDispatcher];
        };

    hostLinkerEnvFor = buildPackageSet': let
      target =
        buildPackageSet'.stdenv.buildPlatform.rust.cargoEnvVarTarget;
      targetUpper = lib.toUpper (lib.replaceStrings ["-"] ["_"] target);
      linkerEnv = "CARGO_TARGET_${targetUpper}_LINKER";
      hostCc = "${buildPackageSet'.stdenv.cc}/bin/cc";
    in {
      ${linkerEnv} = hostCc;
      "CC_${targetUpper}" = hostCc;
      HOST_CC = hostCc;
      CC_FOR_BUILD = hostCc;
      # Crane serializes direct Cargo arguments as __CRANE_EXPORT_* values.
      # Keep those exports aligned too: an env-only override is deliberately
      # filtered when the original derivation already has the direct name,
      # but the setup hook consumes the export form for cargoArtifacts.
      "__CRANE_EXPORT_${linkerEnv}" = hostCc;
      "__CRANE_EXPORT_CC_${targetUpper}" = hostCc;
      __CRANE_EXPORT_HOST_CC = hostCc;
      __CRANE_EXPORT_CC_FOR_BUILD = hostCc;
    };

    withCrossRust = {
      package,
      buildPackageSet' ? buildPackages,
      enable ? true,
    }:
      if !enable
      then package
      else
        withRustCache {
          inherit package;
          linkerPackageSet = buildPackageSet';
        };

    withCrossLinker = {
      package,
      buildPackageSet' ? buildPackages,
      enable ? true,
    }:
      if !enable
      then package
      else let
        linkerEnv = hostLinkerEnvFor buildPackageSet';
      in
        withEnv {
          inherit package;
          env = linkerEnv;
          directEnvOverrides = linkerEnv;
          nativeInputs = [];
          # Linker selection is only the first half of canix's Crossbow
          # composition.  Do not claim that the cache wrapper and setup
          # hooks are present: the following withRustCache call must still
          # install those inputs.
          markCacheWrapped = false;
        };

    withCmakeCache = {
      package,
      enable ? true,
      packageSet ? buildPackages,
    }:
      if !enable
      then package
      else let
        cmakeWrapper = markSandboxWrapper (mkSandboxWrapper packageSet);
        launcher = "${cmakeWrapper}/bin/rs-harbor-sandbox-sccache";
        launcherEnv = {
          CMAKE_C_COMPILER_LAUNCHER = launcher;
          CMAKE_CXX_COMPILER_LAUNCHER = launcher;
        };
      in
        package.overrideAttrs (old: let
          oldEnv = old.env or {};
          conflicting = builtins.filter (name:
            builtins.hasAttr name old
            || builtins.hasAttr name oldEnv
            || builtins.any (flag:
              builtins.isString flag
              && (lib.hasPrefix "-D${name}=" flag || lib.hasPrefix "-D${name}:" flag))
            ((old.cmakeFlags or []) ++ (old.configureFlags or []))) [
            "CMAKE_C_COMPILER_LAUNCHER"
            "CMAKE_CXX_COMPILER_LAUNCHER"
          ];
        in
          if (old.passthru or {}).rsHarborCmakeCacheWrapped or false
          then {}
          else if conflicting != []
          then throw "rs-harbor: package already defines a CMake compiler launcher: ${lib.concatStringsSep ", " conflicting}"
          else {
            nativeBuildInputs = (old.nativeBuildInputs or []) ++ [cmakeWrapper];
            env = (builtins.removeAttrs oldEnv (remoteCacheEnvNames ++ ["RUSTC_WRAPPER" "XDG_CACHE_HOME"])) // launcherEnv;
            passthru =
              (old.passthru or {})
              // {
                rsHarborCmakeCacheWrapped = true;
                rsHarborCmakeCacheWrapper = wrapper;
                rsHarborBuildCacheContract = {
                  inherit namespace sccacheVersion executionModel;
                };
              };
          });

    mkOverlay = {
      rustPackages ? [],
      cmakePackages ? [],
      crossOnly ? false,
    }: final: prev: let
      cross = prev.stdenv.buildPlatform.system != prev.stdenv.hostPlatform.system;
      applyRust = name: acc:
        if !(prev ? ${name}) || (crossOnly && !cross)
        then acc
        else acc // {${name} = withRustCache {package = prev.${name};};};
      applyCmake = name: acc:
        if !(prev ? ${name}) || (crossOnly && !cross)
        then acc
        else
          acc
          // {
            ${name} = withCmakeCache {
              package = prev.${name};
              packageSet = final.buildPackages;
            };
          };
    in
      (builtins.foldl' (name: acc: applyRust name acc) {} rustPackages)
      // (builtins.foldl' (name: acc: applyCmake name acc) {} cmakePackages);
  in {
    inherit namespace sharedCacheDir wrapper wrapperPath dioxusDispatcher dioxusDispatcherPath rustEnv dioxusEnv;
    inherit withRustCache withDioxusCache withCrossRust withCrossLinker withCmakeCache mkOverlay;
    contract = {
      schemaVersion = 2;
      telemetrySchemaVersion = 1;
      telemetryMarker = "RS_HARBOR_SCCACHE_STATS_V1";
      inherit namespaceScope namespaceGeneration namespace sccacheVersion executionModel redisSocketPath;
      compiler = compilerName;
      rustToolchain = contractToolchain // {version = compilerName;};
    };
  };
in {
  inherit mkBuildCachePolicy;
}
