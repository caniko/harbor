{
  pkgs,
  sccacheDefault,
  sccachePackage,
  serviceName ? "sccache-user-daemon.service",
}:
pkgs.writeShellScriptBin "sccache-user-daemon-client" ''
  set -eu

  runtime_dir="''${XDG_RUNTIME_DIR:-/run/user/$(${pkgs.coreutils}/bin/id -u)}"
  export XDG_RUNTIME_DIR="$runtime_dir"
  if [ -z "''${DBUS_SESSION_BUS_ADDRESS:-}" ]; then
    export DBUS_SESSION_BUS_ADDRESS="unix:path=$runtime_dir/bus"
  fi
  socket_path="$runtime_dir/${sccacheDefault.userSocketRel}"
  cache_dir="''${XDG_CACHE_HOME:-$HOME/.cache}/${sccacheDefault.userCacheRel}"

  service_ready() {
    ${pkgs.systemd}/bin/systemctl --user is-active --quiet ${serviceName} \
      && [ -S "$socket_path" ]
  }

  if ! service_ready; then
    if ! ${pkgs.coreutils}/bin/timeout ${toString (sccacheDefault.startupTimeout + 5)}s \
      ${pkgs.systemd}/bin/systemctl --user start ${serviceName}; then
      echo "sccache-user-daemon-client: managed sccache service could not start; build refused" >&2
      ${pkgs.systemd}/bin/systemctl --user status ${serviceName} --no-pager -l >&2 || true
      exit 75
    fi
  fi

  if ! service_ready; then
    echo "sccache-user-daemon-client: managed sccache socket is not ready; build refused" >&2
    ${pkgs.systemd}/bin/journalctl --user-unit=${serviceName} --no-pager -n 80 >&2 || true
    exit 75
  fi

  export SCCACHE_SERVER_UDS="$socket_path"
  export SCCACHE_DIR="$cache_dir"
  export CARGO_INCREMENTAL="0"
  exec ${sccachePackage}/bin/sccache "$@"
''
