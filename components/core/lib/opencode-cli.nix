# Profile-driven factory for the `harbor-opencode` CLI. Everything
# language-specific enters through `profiles` (the registry contract is
# documented in ./opencode.nix); this file owns only generic engine
# behaviour: project detection, `--kind` resolution, render dispatch,
# config ownership validation, and the sync/check/detect/rollout commands.
#
# `cliScript` returns the generated script text purely (evaluable with
# `nix eval --raw .#lib --apply 'l: l.opencode.cliScript {}'` for
# bash -n / shellcheck testing); `mkCli` packages it through
# `writeShellApplication`, which adds the runtime PATH and runs
# shellcheck plus a syntax check over the result at build time.
{
  lib,
  opencode,
}: let
  markerKey = opencode.formatPolicy.marker;
  markerValue = opencode.formatPolicy.markerValue;
  inherit (opencode) configPath;

  scriptFor = profiles: let
    registry = opencode.mkRegistry profiles;
    names = builtins.attrNames registry;
    subsets = opencode.profileSubsets names;
    # The policy-only (`none`) render arm goes last for readability; all
    # other subsets appear in registry order.
    orderedSubsets = lib.filter (subset: subset != []) subsets ++ [[]];

    # Every accepted `--kind` spelling: the engine keywords, each single
    # profile, and every multi-profile combination.
    kindChoices = lib.concatStringsSep "|" (
      ["detect" "none"]
      ++ names
      ++ map opencode.profileKey (lib.filter (subset: builtins.length subset >= 2) subsets)
    );

    profileNamesLine = "profile_names=(${lib.concatStringsSep " " names})";

    # One `KEY:<openpencil>)` case arm per subset, config pre-rendered at
    # evaluation time so the script carries no runtime JSON generator.
    renderCases = lib.concatStrings (
      lib.concatMap (
        subset: let
          lsp = opencode.lspFor registry subset;
          key = opencode.profileKey subset;
        in
          map (
            openpencil: ''
              ${key}:${
                if openpencil
                then "1"
                else "0"
              }) printf '%s' ${lib.escapeShellArg (opencode.configTextFor {inherit lsp openpencil;})} ;;
            ''
          )
          [false true]
      )
      orderedSubsets
    );

    detectCases = lib.concatMapStrings detectCase names;

    # Detection per profile: any registered `detect.files` entry in the
    # project root, or any `detect.flakeMarkers` pattern matching the
    # project's flake.nix. A profile with neither signal never matches
    # `--kind detect`. `matched` accumulates in registry order so the
    # detected key is always canonical.
    detectCase = name: let
      profile = registry.${name};
      files = profile.detect.files;
      markers = profile.detect.flakeMarkers;
      fileCond = lib.concatMapStringsSep " || " (file: ''[ -f "$dir"/${lib.escapeShellArg file} ]'') files;
      flakeCond =
        lib.optionalString (markers != [])
        ''{ [ -f "$dir/flake.nix" ] && grep -Eq ${lib.escapeShellArg (lib.concatStringsSep "|" markers)} "$dir/flake.nix"; }'';
      condition =
        if files != [] && markers != []
        then "${fileCond} || ${flakeCond}"
        else if files != []
        then fileCond
        else if markers != []
        then flakeCond
        else null;
    in
      lib.optionalString (condition != null) ''
        if ${condition}; then
          matched="''${matched:+$matched,}${name}"
        fi
      '';
  in ''
    set -euo pipefail

    usage() {
      cat <<'USAGE'
    Usage:
      harbor-opencode sync --kind KIND [--openpencil] [--force] [--root DIR]
      harbor-opencode check --kind KIND [--openpencil] [--root DIR]
      harbor-opencode detect [--root DIR]
      harbor-opencode rollout [--root DIR] [--check] [--force]

      KIND is one of: ${kindChoices}
      `detect` (default) picks profiles from the project's files, `none`
      renders the policy-only config, and a comma-joined profile list
      picks an explicit combination (render keys are registry-ordered,
      so `b,a` and `a,b` render the same config).

    rollout treats every git repository under --root as a project;
    profile detection falls back to `none` (a policy-only config) when
    no profile matches. Dirty working trees are coordination-blocked
    and reported separately; --check inspects their config state
    read-only. A config that lacks the ${markerKey} marker — or whose
    marker is not a top-level key with value "${markerValue}" — is
    treated as hand-written and is never overwritten without --force.
    USAGE
    }

    die() {
      printf 'harbor-opencode: %s\n' "$*" >&2
      exit 1
    }

    root="."
    kind="detect"
    check_only=0
    openpencil=0
    force=0

    marker_key='${markerKey}'
    marker_value='${markerValue}'
    kind_choices='${kindChoices}'
    ${profileNamesLine}

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
          --openpencil)
            openpencil=1
            shift
            ;;
          --force)
            force=1
            shift
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

    detect_key() {
      local dir="$1"
      local matched=""
      ${detectCases}
      if [ -n "$matched" ]; then
        printf '%s\n' "$matched"
      else
        printf 'none\n'
      fi
    }

    # Validate an explicit `--kind` value: every comma-separated token
    # must be a registered profile (`none` is only valid alone,
    # `detect` is handled by resolve_key), then re-emit the tokens in
    # registry order so `--kind b,a` and detection share one render key.
    normalize_kind() {
      local out="" tok name known
      local -a toks=()
      IFS=',' read -r -a toks <<< "$kind"
      [ "''${#toks[@]}" -gt 0 ] || die "unsupported kind: $kind (expected one of: $kind_choices)"
      for tok in "''${toks[@]}"; do
        [ -n "$tok" ] || die "empty profile in --kind: $kind"
        if [ "$tok" = "none" ]; then
          die "'none' cannot be combined with profiles: $kind"
        fi
        known=0
        for name in "''${profile_names[@]}"; do
          if [ "$tok" = "$name" ]; then
            known=1
            break
          fi
        done
        if [ "$known" -ne 1 ]; then
          die "unsupported kind: $tok (expected one of: $kind_choices)"
        fi
      done
      for name in "''${profile_names[@]}"; do
        for tok in "''${toks[@]}"; do
          if [ "$tok" = "$name" ]; then
            out="''${out:+$out,}$name"
            break
          fi
        done
      done
      printf '%s\n' "$out"
    }

    resolve_key() {
      local dir="$1"
      case "$kind" in
        detect) detect_key "$dir" ;;
        none) printf 'none\n' ;;
        *) normalize_kind ;;
      esac
    }

    extra_flags() {
      if [ "$openpencil" -eq 1 ]; then
        printf ' --openpencil'
      fi
    }

    label() {
      if [ "$openpencil" -eq 1 ]; then
        printf '%s+openpencil\n' "$1"
      else
        printf '%s\n' "$1"
      fi
    }

    render_config() {
      # shellcheck disable=SC2016 # JSON literals below contain intentional $ kept verbatim.
      case "$1:$openpencil" in
    ${renderCases}
        *) die "unsupported render key: $1" ;;
      esac
    }

    config_path() {
      printf '%s\n' "$1/${configPath}"
    }

    # Atomic replace: render to a temp file next to the target, then
    # rename over it, so a crashed or concurrent write can never leave a
    # half-written config behind. Creates the target directory itself.
    write_config() {
      local key="$1" path="$2"
      local dir tmp
      dir="$(dirname "$path")"
      mkdir -p "$dir"
      tmp="$(mktemp "$dir/$(basename "$path").tmp.XXXXXX")"
      render_config "$key" > "$tmp"
      mv -f "$tmp" "$path"
    }

    # Classify the config under a project dir:
    #   missing | ok | stale | custom
    # The marker must be a top-level key with the exact value: a
    # hand-written config that merely mentions the marker key (in a
    # comment, a nested object, or with another value) is still
    # `custom`, and JSONC that cannot be parsed fails closed as
    # `custom`. A differing file that carries a valid marker is merely
    # `stale` (an older harbor render, or the same document with comments)
    # and is safe to overwrite. Ownership parsing is JSONC-aware: `//`
    # line comments and `/* */` block comments outside strings are
    # replaced with whitespace before the top-level marker check, so a
    # valid marked config with comments is `stale`, not `custom`.
    # Comments never join tokens: `tru/**/e` stays two fragments and
    # fails parsing. Trailing commas stay unparsable and fail closed
    # as `custom`.
    #
    # The stripper below respects double-quoted strings and backslash
    # escapes, so marker-looking text inside a string value can never
    # satisfy the ownership check.
    has_marker() {
      local path="$1"
      python3 - "$path" "$marker_key" "$marker_value" <<'PY' >/dev/null 2>&1
    import json
    import sys
    path, key, want = sys.argv[1], sys.argv[2], sys.argv[3]
    try:
        src = open(path, encoding="utf-8").read()
    except OSError:
        sys.exit(1)
    out = []
    i, n = 0, len(src)
    in_string = False
    escaped = False
    while i < n:
        c = src[i]
        nxt = src[i + 1] if i + 1 < n else ""
        if in_string:
            out.append(c)
            if escaped:
                escaped = False
            elif c == "\\":
                escaped = True
            elif c == '"':
                in_string = False
            i += 1
            continue
        if c == '"':
            in_string = True
            out.append(c)
            i += 1
            continue
        if c == "/" and nxt == "/":
            while i < n and src[i] != "\n":
                i += 1
            continue
        if c == "/" and nxt == "*":
            i += 2
            closed = False
            newlines = 0
            while i < n:
                if src[i] == "*" and i + 1 < n and src[i + 1] == "/":
                    closed = True
                    break
                if src[i] == "\n":
                    newlines += 1
                i += 1
            if not closed:
                sys.exit(1)
            i += 2
            # JSONC comments are whitespace, never token joiners: replace
            # the comment with a space (plus any newlines it contained) so
            # `tru/**/e` cannot become `true`.
            out.append(" ")
            for _ in range(newlines):
                out.append("\n")
            continue
        out.append(c)
        i += 1
    try:
        doc = json.loads("".join(out))
    except ValueError:
        sys.exit(1)
    if not isinstance(doc, dict):
        sys.exit(1)
    sys.exit(0 if doc.get(key) == want else 1)
    PY
    }
    config_state() {
      local dir="$1"
      local path tmp resolved
      path="$(config_path "$dir")"
      if [ ! -f "$path" ]; then
        printf 'missing\n'
        return 0
      fi
      if ! has_marker "$path"; then
        printf 'custom\n'
        return 0
      fi
      resolved="$(resolve_key "$dir")"
      tmp="$(mktemp)"
      render_config "$resolved" > "$tmp"
      if diff -q "$tmp" "$path" >/dev/null 2>&1; then
        printf 'ok\n'
      else
        printf 'stale\n'
      fi
      rm -f "$tmp"
    }

    # Coordination-blocked only for changes the tool does not own: our
    # own rendered config (and write temp) sitting uncommitted between
    # rollout and the operator's commit is expected dirt, everything
    # else belongs to another session. `--untracked-files=all` lists
    # the files inside an untracked .opencode/ directory individually,
    # so a repeated rollout is not blocked by its own previous write; a
    # git that cannot answer is treated as foreign dirt (fail closed).
    foreign_dirty() {
      local dir="$1"
      local out line stripped git_failed=0
      [ -e "$dir/.git" ] || return 1
      out="$(git -C "$dir" status --porcelain --untracked-files=all 2>/dev/null)" || git_failed=1
      if [ "$git_failed" -ne 0 ]; then
        printf '%s: git status failed; treating working tree as foreign dirt\n' "$dir" >&2
        return 0
      fi
      while IFS= read -r line; do
        [ -n "$line" ] || continue
        stripped="''${line:3}"
        case "$stripped" in
          ${configPath}|${configPath}.tmp|${configPath}.tmp.*) ;;
          *) return 0 ;;
        esac
      done <<< "$out"
      return 1
    }

    sync_one() {
      local dir="$1"
      local resolved path state
      resolved="$(resolve_key "$dir")"
      path="$(config_path "$dir")"
      state="$(config_state "$dir")"
      if [ "$state" = "ok" ]; then
        printf '%s: ok %s\n' "$dir" "$(label "$resolved")"
        return 0
      fi
      if [ "$state" = "custom" ] && [ "$force" -eq 0 ]; then
        printf '%s: %s is a custom config (missing %s marker); refusing to replace it — rerun with --force to overwrite\n' \
          "$dir" "$path" "$marker_key" >&2
        return 1
      fi
      write_config "$resolved" "$path"
      printf '%s: synced %s\n' "$dir" "$(label "$resolved")"
    }

    check_one() {
      local dir="$1"
      local resolved path state
      resolved="$(resolve_key "$dir")"
      path="$(config_path "$dir")"
      state="$(config_state "$dir")"
      case "$state" in
        ok)
          printf '%s: ok %s\n' "$dir" "$(label "$resolved")"
          ;;
        missing)
          printf '%s: %s is missing; run harbor-opencode sync --kind %s%s --root %s\n' \
            "$dir" "$path" "$resolved" "$(extra_flags)" "$dir" >&2
          return 1
          ;;
        stale)
          printf '%s: %s is stale; run harbor-opencode sync --kind %s%s --root %s\n' \
            "$dir" "$path" "$resolved" "$(extra_flags)" "$dir" >&2
          return 1
          ;;
        custom)
          printf '%s: %s is a custom config (missing %s marker) and does not match the harbor render; review it, then replace it with sync --force\n' \
            "$dir" "$path" "$marker_key" >&2
          return 1
          ;;
      esac
    }

    rollout() {
      local scan_root="$1"
      local total=0 ok=0 synced=0 unchanged=0 stale=0 missing=0 custom=0 blocked=0 dirty=0
      local dir path state resolved dirty_note
      local -a dirs=()

      # Every git repository under the scan root is a project: config
      # ownership is language-independent, so profile detection falls
      # back to `none` (policy-only config) instead of skipping repos.
      while IFS= read -r git_entry; do
        dirs+=("$(dirname "$git_entry")")
      done < <(find "$scan_root" -mindepth 1 -maxdepth 3 -name .git -print | sort)

      [ "''${#dirs[@]}" -gt 0 ] || die "no git repositories found under $scan_root"

      for dir in "''${dirs[@]}"; do
        total=$((total + 1))
        path="$(config_path "$dir")"
        dirty_note=""
        if foreign_dirty "$dir"; then
          dirty=$((dirty + 1))
          dirty_note=" (dirty)"
          if [ "$check_only" -eq 0 ]; then
            # Coordination-blocked: reported separately, never permanently
            # excluded — the next rollout picks the repo up once it is clean.
            blocked=$((blocked + 1))
            printf '%s: blocked (dirty working tree)\n' "$dir" >&2
            continue
          fi
        fi

        state="$(config_state "$dir")"
        if [ "$check_only" -eq 1 ]; then
          case "$state" in
            ok)
              ok=$((ok + 1))
              printf '%s: ok%s\n' "$dir" "$dirty_note"
              ;;
            stale)
              stale=$((stale + 1))
              printf '%s: stale%s\n' "$dir" "$dirty_note" >&2
              ;;
            missing)
              missing=$((missing + 1))
              printf '%s: missing%s\n' "$dir" "$dirty_note" >&2
              ;;
            custom)
              custom=$((custom + 1))
              printf '%s: custom config (missing %s marker)%s\n' "$dir" "$marker_key" "$dirty_note" >&2
              ;;
          esac
        else
          if [ "$state" = "custom" ] && [ "$force" -eq 0 ]; then
            custom=$((custom + 1))
            printf '%s: custom config (missing %s marker); skipped, rerun with --force to overwrite\n' \
              "$dir" "$marker_key" >&2
            continue
          fi
          if [ "$state" = "ok" ]; then
            # Already in sync: report without touching the file.
            unchanged=$((unchanged + 1))
            printf '%s: ok%s\n' "$dir" "$dirty_note"
          else
            resolved="$(resolve_key "$dir")"
            write_config "$resolved" "$path"
            synced=$((synced + 1))
            printf '%s: synced %s%s\n' "$dir" "$(label "$resolved")" "$dirty_note"
          fi
        fi
      done

      if [ "$check_only" -eq 1 ]; then
        printf 'harbor-opencode rollout: total=%d ok=%d stale=%d missing=%d custom=%d dirty=%d\n' \
          "$total" "$ok" "$stale" "$missing" "$custom" "$dirty"
        [ $((stale + missing + custom)) -eq 0 ] || exit 1
      else
        printf 'harbor-opencode rollout: total=%d synced=%d unchanged=%d blocked-dirty=%d custom=%d dirty=%d\n' \
          "$total" "$synced" "$unchanged" "$blocked" "$custom" "$dirty"
        [ $((blocked + custom)) -eq 0 ] || exit 1
      fi
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
        detect_key "$root"
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
in {
  cliScript = {profiles ? {}}: scriptFor profiles;

  # Package the generated script. python3 joins the runtime inputs because
  # config ownership validation parses the target config as JSONC; jq stays
  # for the contract checks that assert on rendered JSON fragments.
  mkCli = {
    pkgs,
    profiles ? {},
    name ? "harbor-opencode",
  }:
    pkgs.writeShellApplication {
      inherit name;
      runtimeInputs = [
        pkgs.coreutils
        pkgs.diffutils
        pkgs.findutils
        pkgs.gnugrep
        pkgs.git
        pkgs.jq
        pkgs.python3
      ];
      text = scriptFor profiles;
    };
}
