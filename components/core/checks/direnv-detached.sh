#!/usr/bin/env bash
# Drive the packaged use_flake through interrupted, joined and cached
# activations. Usage: direnv-detached.sh DIRENVRC FIXTURE PHASE
set -euo pipefail
direnvrc=$1 fixture=$2 phase=$3
[[ -d $HOME/fx ]] || cp -r --no-preserve=mode "$fixture" "$HOME/fx"
cd "$HOME/fx"
trap 'tail -n 40 ./*.out .direnv/*.log >&2' ERR

# A gate instead of a slow build: print-dev-env waits until $HOME/release
# exists, so the test decides how long the realization stays in flight.
if [[ ! -x $HOME/bin/nix ]]; then
  mkdir -p "$HOME/bin"
  printf '%s\n' '#!/bin/sh' \
    'case " $* " in *" print-dev-env "*) until [ -e "$HOME/release" ]; do sleep 0.2; done ;; esac' \
    "exec $(command -v nix) \"\$@\"" >"$HOME/bin/nix"
  chmod +x "$HOME/bin/nix"
fi
export PATH=$HOME/bin:$PATH

activate() {
  bash -c 'eval "$(direnv stdlib)"; source "$1"; use flake .; printf "INPUT=%s\n" "$INPUT"' _ "$direnvrc"
}
units() { systemctl --user list-units 'nix-direnv-*' --state=active --no-legend | grep -c . || true; }

case $phase in
interrupt)
  rm -f "$HOME/release"
  # A failed unit degrades the user manager; detection must still detach.
  systemd-run --user --unit=degrade-manager false || true
  set -m
  activate >interrupted.out 2>&1 &
  pid=$!
  until (($(units) == 1)); do sleep 1; done
  # The same signal harbor-llm uses on timeout: SIGKILL the whole group.
  kill -KILL -- "-$pid"
  wait "$pid" || true
  (($(units) == 1))
  printf 'Activation killed; realization still running\n'
  touch "$HOME/release"
  until (($(units) == 0)); do sleep 1; done
  # The detached realization itself must have succeeded and left its root.
  [[ $(cat .direnv/flake-tmp-profile*.status) == 0 ]]
  compgen -G '.direnv/flake-tmp-profile-*-link' >/dev/null
  ;;
renew)
  activate >renew.out 2>&1
  grep -F 'Renewed cache' renew.out
  ;;
cached)
  activate >cached.out 2>&1
  grep -F 'Using cached dev shell' cached.out
  ;;
concurrent)
  rm -f "$HOME/release"
  sed -i 's/input-v1/input-v2/' flake.nix
  activate >first.out 2>&1 &
  first=$!
  activate >second.out 2>&1 &
  second=$!
  until (($(units) == 1)); do sleep 1; done
  sleep 3
  # The second activation waits on the profile lock; it starts no second unit.
  (($(units) == 1))
  touch "$HOME/release"
  wait "$first"
  wait "$second"
  [[ $(grep '^INPUT=' first.out) == "$(grep '^INPUT=' second.out)" ]]
  grep -F 'input-v2' first.out
  ;;
fallback)
  touch "$HOME/release"
  sed -i 's/input-v[0-9]/input-root/' flake.nix
  activate >fallback.out 2>&1
  grep -F 'Renewed cache' fallback.out
  if grep -F 'realizing in user unit' fallback.out; then exit 1; fi
  ;;
*) exit 2 ;;
esac
printf 'phase %s passed\n' "$phase"
