#!/usr/bin/env bash
# android-device — enroll and verify a physical Android test device.
#
# This is the canonical source of the helper packaged by
# `harbor-android.lib.mkAndroidDeviceTools` (lib/android-device.nix reads this
# file). Device definitions are local runtime JSON, never Nix inputs: no
# serial enters the Nix store through this helper.
#
#   android-device enroll --serial SERIAL --out FILE [--product PRODUCT] [--adb ADB]
#   android-device verify --definition FILE [--adb ADB]
#
# enroll writes {schemaVersion, adbSerial, product, model} with mode 0600 and
# refuses to overwrite an existing file. verify checks the definition against
# the currently connected USB device and prints the serial on stdout.
# Authorization (which user/profile may be touched) is always the caller's
# decision, never part of the definition.
set -euo pipefail
adb="${ADB:-adb}"
# Per-command ceiling so a wedged transport fails closed instead of hanging
# the caller. Overridable for slow hosts; 0 disables when `timeout` exists.
adb_timeout="${ANDROID_DEVICE_TIMEOUT:-90}"

adb_run() {
  if [[ "$adb_timeout" != "0" ]] && command -v timeout >/dev/null 2>&1; then
    timeout "$adb_timeout" "$adb" "$@"
  else
    "$adb" "$@"
  fi
}

usage() {
  echo "usage: android-device enroll --serial SERIAL --out FILE [--product PRODUCT] [--adb ADB]" >&2
  echo "       android-device verify --definition FILE [--adb ADB]" >&2
  exit 2
}

device_state() { # serial -> prints the exact transport state field or fails
  local serial="$1" states count
  states="$(adb_run devices -l | awk -v s="$serial" 'NR>1 && $1==s {print $2}')"
  count="$(printf '%s\n' "$states" | grep -c . || true)"
  [[ "$count" == "1" ]] || { echo "android-device: refusing: serial $serial is not uniquely connected" >&2; return 1; }
  printf '%s\n' "$states"
}

device_line() { # serial -> prints the `adb devices -l` line or fails
  local serial="$1" state line
  state="$(device_state "$serial")" || return 1
  [[ "$state" == "device" ]] || { echo "android-device: refusing: serial $serial is not authorized/online (state: $state)" >&2; return 1; }
  line="$(adb_run devices -l | awk -v s="$serial" 'NR>1 && $1==s {print}')"
  [[ "$line" == *"usb:"* ]] || { echo "android-device: refusing: serial $serial is not a USB device" >&2; return 1; }
  printf '%s\n' "$line"
}

getprop() { # serial key -> value without carriage returns
  adb_run -s "$1" shell getprop "$2" | tr -d '\r'
}

cmd_enroll() {
  local serial="" out="" product=""
  while [[ "$#" -gt 0 ]]; do
    case "$1" in
      --serial) serial="${2:-}"; shift 2 ;;
      --out) out="${2:-}"; shift 2 ;;
      --product) product="${2:-}"; shift 2 ;;
      --adb) adb="${2:-}"; shift 2 ;;
      *) usage ;;
    esac
  done
  [[ -n "$serial" ]] || { echo "android-device: refusing: --serial is required" >&2; exit 1; }
  [[ -n "$out" ]] || { echo "android-device: refusing: --out is required" >&2; exit 1; }
  [[ ! -e "$out" ]] || { echo "android-device: refusing: $out exists (remove it explicitly to re-enroll)" >&2; exit 1; }
  device_line "$serial" >/dev/null
  local observed_product model
  observed_product="$(getprop "$serial" ro.product.device)"
  [[ -n "$observed_product" ]] || { echo "android-device: refusing: empty product for $serial" >&2; exit 1; }
  if [[ -n "$product" && "$product" != "$observed_product" ]]; then
    echo "android-device: refusing: product $observed_product does not match --product $product" >&2
    exit 1
  fi
  model="$(getprop "$serial" ro.product.model)"
  umask 077
  local tmp
  tmp="$(mktemp "${out}.tmp.XXXXXX")"
  trap 'rm -f "$tmp"' RETURN
  printf '{"schemaVersion":1,"adbSerial":%s,"product":%s,"model":%s}\n' \
    "$(jq -Rn --arg v "$serial" '$v')" \
    "$(jq -Rn --arg v "$observed_product" '$v')" \
    "$(jq -Rn --arg v "$model" '$v')" >"$tmp"
  chmod 0600 "$tmp"
  # Atomic no-overwrite publish: the pre-check above is advisory only.
  if ! (set -o noclobber; cat "$tmp" >"$out") 2>/dev/null; then
    if [[ -e "$out" ]]; then
      echo "android-device: refusing: $out exists (remove it explicitly to re-enroll)" >&2
    else
      echo "android-device: refusing: cannot write $out" >&2
    fi
    return 1
  fi
  echo "android-device: enrolled $observed_product (${model:-unknown model}) as $serial" >&2
}

cmd_verify() {
  local def=""
  while [[ "$#" -gt 0 ]]; do
    case "$1" in
      --definition) def="${2:-}"; shift 2 ;;
      --adb) adb="${2:-}"; shift 2 ;;
      *) usage ;;
    esac
  done
  [[ -n "$def" && -f "$def" ]] || { echo "android-device: refusing: definition not found: ${def:-}" >&2; exit 1; }
  jq -e '.schemaVersion == 1
    and (.adbSerial | type) == "string" and (.adbSerial | length) > 0
    and (.product | type) == "string" and (.product | length) > 0
    and (.model == null or ((.model | type) == "string"))' "$def" >/dev/null 2>&1 \
    || { echo "android-device: refusing: $def is not a valid device definition" >&2; exit 1; }
  local serial product
  serial="$(jq -r '.adbSerial' "$def")"
  product="$(jq -r '.product' "$def")"
  device_line "$serial" >/dev/null
  local observed
  observed="$(getprop "$serial" ro.product.device)"
  [[ "$observed" == "$product" ]] || {
    echo "android-device: refusing: connected product ${observed:-unknown} does not match definition $product" >&2
    exit 1
  }
  printf '%s\n' "$serial"
}

[[ "${1:-}" == "enroll" ]] && { shift; cmd_enroll "$@"; exit 0; }
[[ "${1:-}" == "verify" ]] && { shift; cmd_verify "$@"; exit 0; }
usage
