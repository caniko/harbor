# Small, shared systemd helpers for sccache's Unix-socket lifecycle.
{pkgs}: let
  coreutils = pkgs.coreutils;
in {
  # Repair only the socket entry. A historical configuration accidentally
  # created the socket path as a directory; remove it only when it is empty.
  repairSocket = pkgs.writeShellScript "rs-harbor-sccache-repair-socket" ''
    set -eu
    socket="$1"
    if [ -d "$socket" ] && [ ! -L "$socket" ]; then
      ${coreutils}/bin/rmdir "$socket"
    fi
    ${coreutils}/bin/rm -f "$socket"
  '';

  # sccache creates its socket only after the backend has passed startup
  # checks, so socket creation is the service readiness boundary.
  waitForSocket = pkgs.writeShellScript "rs-harbor-sccache-wait-for-socket" ''
    set -eu
    socket="$1"
    timeout_seconds="$2"
    deadline=$(( $(${coreutils}/bin/date +%s) + timeout_seconds ))

    while [ ! -S "$socket" ]; do
      if [ "$(${coreutils}/bin/date +%s)" -ge "$deadline" ]; then
        echo "rs-harbor sccache: timed out waiting for $socket" >&2
        exit 1
      fi
      ${coreutils}/bin/sleep 0.1
    done
  '';
}
