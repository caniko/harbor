#!/usr/bin/env bash
# Exercise the complete packaged function with deterministic Nix responses.
set -Eeuo pipefail
trap 'status=$?; printf "Packaged renewal failed at line %s: %s (status %s)\n" "$LINENO" "$BASH_COMMAND" "$status" >&2; exit "$status"' ERR
source "$1"
calls=0
profile_calls=0
_nix() {
  case "$1" in
  print-dev-env)
    touch "$tmp_profile"
    printf 'renewed\n'
    ;;
  flake)
    [[ $2 == archive ]]
    printf '%s\n' "$input_json"
    return "${archive_failure:-0}"
    ;;
  build)
    calls=$((calls + 1))
    args=("$@")
    [[ $(cat "$profile_rc") == old ]]
    if [[ ${REAL_ROOTS:-0} == 1 ]]; then
      "$NIX_ROOT_BINARY" "$@"
      return
    fi
    return "${root_failure:-0}"
    ;;
  *) return 99 ;;
  esac
}
_nix_direnv_info() { :; }
_nix_direnv_preflight() { :; }
watch_file() { :; }
direnv_layout_dir() { printf '%s\n' "$layout"; }
_nix_argsum_suffix() { printf '%s\n' '-fixture'; }
_nix_direnv_watches() {
  local -n watched=$1
  watched=()
}
_nix_add_gcroot() {
  profile_calls=$((profile_calls + 1))
  [[ ${profile_failure:-0} == 0 ]] || return 1
  touch "$2"
}
_nix_import_env() { cat "$1" >/dev/null; }

layout="$PWD/layout"
mkdir -p "$layout"
profile_rc="$layout/flake-profile-fixture.rc"
renew_cache() {
  rm -f "$layout/flake-profile-fixture"
  use_flake .
}
input_json='["/nix/store/aaaaaaaa-source", "/nix/store/bbbbbbbb-source", "/nix/store/aaaaaaaa-source"]'
if [[ ${REAL_ROOTS:-0} == 1 ]]; then
  first=$("$NIX_STORE_BINARY" --add "$1")
  second=$("$NIX_STORE_BINARY" --add "$0")
  input_json="[\"$first\", \"$second\", \"$first\"]"
fi
printf 'old\n' >"$profile_rc"
renew_cache
[[ $calls == 1 ]]
[[ $(cat "$profile_rc") == renewed ]]
[[ ${args[0]} == build && ${args[1]} == --max-jobs && ${args[2]} == 0 ]]
[[ ${args[3]} == --option && ${args[4]} == builders && ${args[5]} == "" ]]
[[ ${args[6]} == --out-link && ${args[7]} == "$layout/flake-inputs//input-"* && ${args[8]} == -- ]]
[[ ${#args[@]} == 11 ]]
if [[ ${REAL_ROOTS:-0} == 1 ]]; then
  [[ ${args[9]} == "$first" && ${args[10]} == "$second" ]]
  [[ $(readlink "${args[7]}") == "$first" ]]
  [[ $(readlink "${args[7]}-1") == "$second" ]]
  # Nix canonicalizes the doubled separator in nix-direnv's input directory.
  # Normalize the link pathname without following it into the store.
  root_link=$(realpath --canonicalize-missing --no-symlinks "${args[7]}")
  roots=$("$NIX_STORE_BINARY" --query --roots "$first")
  printf '%s\n' "$roots"
  grep -F -- "$root_link" <<<"$roots"
  roots=$("$NIX_STORE_BINARY" --query --roots "$second")
  printf '%s\n' "$roots"
  grep -F -- "$root_link-1" <<<"$roots"
else
  [[ ${args[9]} == /nix/store/aaaaaaaa-source && ${args[10]} == /nix/store/bbbbbbbb-source ]]
fi

# Two warm loads must preserve the cache watch and refresh only root symlinks.
# Make timestamps deterministic rather than sleeping for clock resolution.
printf 'target\n' >"$layout/root-target"
rm -f "$layout/flake-profile-fixture"
ln -s root-target "$layout/flake-profile-fixture"
ln -s root-target "$layout/nix-profile-fixture"
touch -t 200001010000 "$profile_rc" "$layout/root-target"
touch -h -t 200001010000 "$layout/flake-profile-fixture" "$layout/nix-profile-fixture"
cache_timestamp=$(stat -c %y "$profile_rc")
target_timestamp=$(stat -c %y "$layout/root-target")
for attempt in 1 2; do
  use_flake .
  if [[ $(stat -c %y "$profile_rc") != "$cache_timestamp" ]]; then
    printf 'Cached load %s changed the watched cache timestamp\n' "$attempt" >&2
    exit 1
  fi
  [[ $(stat -c %y "$layout/root-target") == "$target_timestamp" ]]
done
[[ $(stat -c %y "$layout/flake-profile-fixture") != "$target_timestamp" ]]
[[ $(stat -c %y "$layout/nix-profile-fixture") != "$target_timestamp" ]]
printf 'Cached loads preserve cache watches and refresh root symlinks\n'

input_json='[]'
printf 'old\n' >"$profile_rc"
renew_cache
[[ $calls == 1 ]]
[[ $(cat "$profile_rc") == renewed ]]
[[ -z $(find "$layout/flake-inputs" -type l -print) ]]

# Failures must propagate even when use_flake is called conditionally, and
# neither archive nor root failure may publish the fresh environment cache.
input_json='["/nix/store/aaaaaaaa-source"]'
root_failure=1
REAL_ROOTS=0
printf 'old\n' >"$profile_rc"
if renew_cache; then
  exit 1
else
  [[ $? == 1 ]]
fi
[[ $(cat "$profile_rc") == old ]]
[[ $calls == 2 ]]

root_failure=0
archive_failure=0
profile_failure=1
if renew_cache; then
  exit 1
else
  [[ $? == 1 ]]
fi
[[ $(cat "$profile_rc") == old ]]
[[ $calls == 3 ]]
[[ $profile_calls == 3 ]]
printf 'Packaged renewal: profile-root failure cannot publish the cache\n'

profile_failure=0
archive_failure=1
if renew_cache; then
  exit 1
else
  [[ $? == 1 ]]
fi
[[ $(cat "$profile_rc") == old ]]
[[ $calls == 3 ]]
[[ $profile_calls == 3 ]]
printf 'Packaged renewal: duplicates, empty input, conditional archive/root failures, previous-cache retention passed\n'
