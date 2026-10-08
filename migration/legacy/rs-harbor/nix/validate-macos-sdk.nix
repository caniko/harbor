{pkgs}:
pkgs.writeShellApplication {
  name = "validate-macos-sdk";
  runtimeInputs = [
    pkgs.coreutils
    pkgs.jq
  ];
  text = ''
    set -euo pipefail

    usage() {
      cat <<'USAGE'
    Usage:
      validate-macos-sdk /path/to/MacOSX26.1.sdk [26.1]

    Validates that a macOS SDK root contains the files and frameworks
    required by rs-harbor's osxcross integration.
    USAGE
    }

    if [ "''${1:-}" = "--help" ] || [ "''${1:-}" = "-h" ]; then
      usage
      exit 0
    fi

    if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
      usage >&2
      exit 64
    fi

    sdk_root="$1"
    expected_version="''${2:-}"
    missing=0

    require_path() {
      if [ ! -e "$sdk_root/$1" ]; then
        echo "missing required SDK entry: $1" >&2
        missing=1
      fi
    }

    require_framework_payload() {
      framework="$1"
      name="$2"
      if [ ! -e "$framework/$name" ] \
        && [ ! -e "$framework/$name.tbd" ] \
        && [ ! -e "$framework/Versions/A/$name" ] \
        && [ ! -e "$framework/Versions/A/$name.tbd" ] \
        && [ ! -e "$framework/Versions/Current/$name" ] \
        && [ ! -e "$framework/Versions/Current/$name.tbd" ]; then
        echo "missing framework binary or .tbd: $framework/$name" >&2
        missing=1
      fi
    }

    if [ ! -d "$sdk_root" ]; then
      echo "SDK root is not a directory: $sdk_root" >&2
      exit 65
    fi

    require_path SDKSettings.json
    require_path usr/include/TargetConditionals.h
    require_path System/Library/Frameworks
    require_path System/Library/Frameworks/SystemConfiguration.framework
    require_path System/Library/Frameworks/CoreFoundation.framework

    require_framework_payload "$sdk_root/System/Library/Frameworks/SystemConfiguration.framework" SystemConfiguration
    require_framework_payload "$sdk_root/System/Library/Frameworks/CoreFoundation.framework" CoreFoundation

    if [ -n "$expected_version" ] && [ -e "$sdk_root/SDKSettings.json" ]; then
      actual_version="$(jq -r '.Version // empty' "$sdk_root/SDKSettings.json")"
      if [ "$actual_version" != "$expected_version" ]; then
        echo "SDK version mismatch: expected $expected_version, got ''${actual_version:-<missing>}" >&2
        missing=1
      fi
    fi

    if [ "$missing" -ne 0 ]; then
      echo "macOS SDK validation failed: $sdk_root" >&2
      exit 66
    fi

    echo "macOS SDK validation passed: $sdk_root"
  '';
}
