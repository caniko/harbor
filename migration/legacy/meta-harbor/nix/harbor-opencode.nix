{
  pkgs,
  lib,
}: let
  script = ''
    set -euo pipefail

    usage() {
      cat <<'USAGE'
    Usage:
      harbor-opencode sync --kind rust|python|mixed|detect [--root DIR]
      harbor-opencode check --kind rust|python|mixed|detect [--root DIR]
      harbor-opencode detect [--root DIR]
      harbor-opencode rollout [--root DIR] [--check]
    USAGE
    }

    die() {
      printf 'harbor-opencode: %s\n' "$*" >&2
      exit 1
    }

    root="."
    kind="detect"
    check_only=0

    parse_common() {
      while [ "$#" -gt 0 ]; do
        case "$1" in
          --kind)
            [ "$#" -ge 2 ] || die "--kind requires a value"
            kind="$2"
            shift 2
            ;;
          --root)
            [ "$#" -ge 2 ] || die "--root requires a value"
            root="$2"
            shift 2
            ;;
          --check)
            check_only=1
            shift
            ;;
          -h|--help)
            usage
            exit 0
            ;;
          *)
            die "unknown argument: $1"
            ;;
        esac
      done
    }

    detect_kind() {
      local dir="$1"
      local has_rust=0
      local has_python=0

      [ -f "$dir/Cargo.toml" ] && has_rust=1
      [ -f "$dir/pyproject.toml" ] && has_python=1

      if [ -f "$dir/flake.nix" ]; then
        grep -Eq 'rs-harbor|mkDevShell|mkDevShells' "$dir/flake.nix" && has_rust=1
        grep -Eq 'py-harbor|mkUvDevShell|mkUvDevShells' "$dir/flake.nix" && has_python=1
      fi

      if [ "$has_rust" -eq 1 ] && [ "$has_python" -eq 1 ]; then
        printf 'mixed\n'
      elif [ "$has_rust" -eq 1 ]; then
        printf 'rust\n'
      elif [ "$has_python" -eq 1 ]; then
        printf 'python\n'
      else
        die "could not detect harbor project kind under $dir"
      fi
    }

    resolve_kind() {
      local dir="$1"
      case "$kind" in
        detect) detect_kind "$dir" ;;
        rust|python|mixed) printf '%s\n' "$kind" ;;
        *) die "unsupported kind: $kind" ;;
      esac
    }

    render_config() {
      case "$1" in
        rust)
          cat <<'JSON'
    {"$schema":"https://opencode.ai/config.json","lsp":{"nixd":{"command":["nixd"]},"rust":{"command":["rust-analyzer"]},"taplo":{"command":["taplo","lsp","stdio"],"extensions":[".toml"]}}}
    JSON
          ;;
        python)
          cat <<'JSON'
    {"$schema":"https://opencode.ai/config.json","lsp":{"basedpyright":{"command":["basedpyright-langserver","--stdio"],"extensions":[".py",".pyi"]},"pyright":{"disabled":true},"ruff":{"command":["ruff","server"],"extensions":[".py",".pyi"]}}}
    JSON
          ;;
        mixed)
          cat <<'JSON'
    {"$schema":"https://opencode.ai/config.json","lsp":{"basedpyright":{"command":["basedpyright-langserver","--stdio"],"extensions":[".py",".pyi"]},"nixd":{"command":["nixd"]},"pyright":{"disabled":true},"ruff":{"command":["ruff","server"],"extensions":[".py",".pyi"]},"rust":{"command":["rust-analyzer"]},"taplo":{"command":["taplo","lsp","stdio"],"extensions":[".toml"]}}}
    JSON
          ;;
        *) die "unsupported kind: $1" ;;
      esac
    }

    config_path() {
      printf '%s/.opencode/opencode.jsonc\n' "$1"
    }

    sync_one() {
      local dir="$1"
      local resolved
      local path
      resolved="$(resolve_kind "$dir")"
      path="$(config_path "$dir")"
      mkdir -p "$(dirname "$path")"
      render_config "$resolved" > "$path"
      printf '%s: synced %s\n' "$dir" "$resolved"
    }

    check_one() {
      local dir="$1"
      local resolved
      local path
      local tmp
      resolved="$(resolve_kind "$dir")"
      path="$(config_path "$dir")"
      [ -f "$path" ] || die "$path is missing; run harbor-opencode sync --kind $resolved --root $dir"
      tmp="$(mktemp)"
      trap 'rm -f "$tmp"' RETURN
      render_config "$resolved" > "$tmp"
      diff -u "$tmp" "$path" >/dev/null || {
        diff -u "$tmp" "$path" >&2 || true
        die "$path is stale; run harbor-opencode sync --kind $resolved --root $dir"
      }
      printf '%s: ok %s\n' "$dir" "$resolved"
    }

    rollout() {
      local scan_root="$1"
      find "$scan_root" -mindepth 1 -maxdepth 3 -name flake.nix -print | while IFS= read -r flake; do
        local dir
        dir="$(dirname "$flake")"
        if grep -Eq 'rs-harbor|py-harbor' "$flake"; then
          if [ "$check_only" -eq 1 ]; then
            kind=detect check_one "$dir"
          else
            kind=detect sync_one "$dir"
          fi
        fi
      done
    }

    [ "$#" -ge 1 ] || {
      usage
      exit 2
    }

    cmd="$1"
    shift

    case "$cmd" in
      sync)
        parse_common "$@"
        sync_one "$root"
        ;;
      check)
        parse_common "$@"
        check_one "$root"
        ;;
      detect)
        parse_common "$@"
        detect_kind "$root"
        ;;
      rollout)
        parse_common "$@"
        rollout "$root"
        ;;
      -h|--help)
        usage
        ;;
      *)
        die "unknown command: $cmd"
        ;;
    esac
  '';
in
  pkgs.writeShellApplication {
    name = "harbor-opencode";
    runtimeInputs = [
      pkgs.coreutils
      pkgs.diffutils
      pkgs.findutils
      pkgs.gnugrep
    ];
    text = script;
  }
