#!/usr/bin/env bash
# Drive the packaged use_flake through interrupted, joined and cached
# activations. Usage: direnv-detached.sh DIRENVRC FIXTURE PHASE
set -euo pipefail
direnvrc=$1 fixture=$2 phase=$3
[[ -d $HOME/fx ]] || cp -r --no-preserve=mode "$fixture" "$HOME/fx"
cd "$HOME/fx"
trap 'tail -n 40 ./*.out >&2' ERR

activate() {
  bash -c 'eval "$(direnv stdlib)"; source "$1"; use flake .; printf "SLOW=%s\n" "$SLOW"' _ "$direnvrc"
}
building() { systemctl --user list-units 'nix-direnv-*' --state=active --no-legend | grep -q .; }

case $phase in
interrupt)
  # A failed unit degrades the user manager; detection must still detach.
  systemd-run --user --unit=degrade-manager false || true
  set -m
  activate >interrupted.out 2>&1 &
  pid=$!
  until building; do sleep 1; done
  # The same signal harbor-llm uses on timeout: SIGKILL the whole group.
  kill -KILL -- "-$pid"
  wait "$pid" || true
  building
  printf 'Activation killed; realization still running\n'
  until ! building; do sleep 1; done
  ;;
renew)
  start=$SECONDS
  activate >renew.out 2>&1
  grep -F 'Renewed cache' renew.out
  # The fixture input sleeps 20 s; a renewal under that did not rebuild it.
  ((SECONDS - start < 20))
  ;;
cached)
  activate >cached.out 2>&1
  grep -F 'Using cached dev shell' cached.out
  ;;
concurrent)
  sed -i 's/slow-input-v1/slow-input-v2/' flake.nix
  start=$SECONDS
  activate >first.out 2>&1 &
  first=$!
  activate >second.out 2>&1
  wait "$first"
  # One shared 20 s build, not two.
  ((SECONDS - start < 35))
  [[ $(grep '^SLOW=' first.out) == "$(grep '^SLOW=' second.out)" ]]
  grep -F 'slow-input-v2' first.out
  ;;
fallback)
  sed -i 's/slow-input-v[0-9]/slow-input-root/' flake.nix
  activate >fallback.out 2>&1
  grep -F 'Renewed cache' fallback.out
  if grep -F 'realizing in user unit' fallback.out; then exit 1; fi
  ;;
*) exit 2 ;;
esac
printf 'phase %s passed\n' "$phase"
