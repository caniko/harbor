#!/usr/bin/env bash
# Hosted qualification only. Each phase leaves evidence before reporting failure.
set -euo pipefail
export LC_ALL=C
export HARBOR_QUALIFICATION_SOURCE
HARBOR_QUALIFICATION_SOURCE=$(git rev-parse --show-toplevel)
cd "$HARBOR_QUALIFICATION_SOURCE"
evidence=${QUALIFICATION_EVIDENCE:-${RUNNER_TEMP:?Set RUNNER_TEMP or QUALIFICATION_EVIDENCE}/rust-musl-openssl-evidence}
mkdir -p "$evidence"
evidence=$(realpath "$evidence")
baseline=acfb79cd3768ec7bc90c3cf1019fe135d50c3d5f
helper=components/rust/lib/cross-packages.nix
check=checks.x86_64-linux.rust-mkCrossPackages-musl-openssl
nix_args=(--no-update-lock-file --max-jobs 1 --cores 2 --print-build-logs)

fail() { echo "ERROR: $*" >&2; exit 1; }

# Nix's failure summary abbreviates logs. Retain the complete log for every
# mentioned derivation, including the outer static-ELF validator when it fails.
collect_logs() {
  local phase=$1 drv name
  mkdir -p "$evidence/$phase/nix-logs"
  { grep -Eo '/nix/store/[a-z0-9]{32}-[a-zA-Z0-9+._?=-]+\.drv' \
      "$evidence/$phase/build.log" || true; } | sort -u > "$evidence/$phase/derivations.txt"
  if [[ -L "$evidence/$phase/result" ]]; then
    nix path-info --derivation "$evidence/$phase/result" \
      >> "$evidence/$phase/derivations.txt" 2> "$evidence/$phase/path-info.stderr"
  fi
  sort -u -o "$evidence/$phase/derivations.txt" "$evidence/$phase/derivations.txt"
  while IFS= read -r drv; do
    [[ -n "$drv" ]] || continue
    name=$(basename "$drv")
    if nix log --log-format raw "$drv" > "$evidence/$phase/nix-logs/$name.log" \
        2> "$evidence/$phase/nix-logs/$name.stderr"; then
      printf '%s\n' "$drv" >> "$evidence/$phase/logs-retained.txt"
    fi
  done < "$evidence/$phase/derivations.txt"
}

build() {
  local phase=$1 status
  shift
  mkdir -p "$evidence/$phase"
  printf '%q ' nix build "${nix_args[@]}" --json --out-link "$evidence/$phase/result" "$@" \
    > "$evidence/$phase/command.txt"
  printf '\n' >> "$evidence/$phase/command.txt"
  if nix build "${nix_args[@]}" --json --out-link "$evidence/$phase/result" "$@" \
      > "$evidence/$phase/result.json" 2> >(tee "$evidence/$phase/build.log" >&2); then
    status=0
  else
    status=$?
  fi
  # Wait for tee to flush before extracting failed derivation paths.
  wait
  printf '%s\n' "$status" > "$evidence/$phase/exit-status"
  collect_logs "$phase"
  return "$status"
}

case ${1:-} in
  prepare)
    git rev-parse HEAD > "$evidence/candidate-revision.txt"
    git rev-parse 'HEAD^{tree}' > "$evidence/candidate-tree.txt"
    git status --porcelain=v1 > "$evidence/candidate-status.txt"
    git archive --format=tar HEAD | gzip -n > "$evidence/source.tar.gz"
    # Full, C-sorted per-file manifest of the exact Git source archive.
    source_dir=$(mktemp -d)
    tar -xzf "$evidence/source.tar.gz" -C "$source_dir"
    (cd "$source_dir"; find . -type f -print0 | sort -z | xargs -0 sha256sum) \
      > "$evidence/fullsource.sha256"
    rm -rf "$source_dir"
    sha256sum flake.lock components/rust/tests/fixtures/musl-openssl/Cargo.lock \
      > "$evidence/locks.sha256"
    jq -n '{repository: env.GITHUB_REPOSITORY, event: env.GITHUB_EVENT_NAME,
      event_sha: env.GITHUB_SHA, candidate_sha: env.QUALIFICATION_HEAD,
      ref: env.GITHUB_REF, run_id: env.GITHUB_RUN_ID, run_attempt: env.GITHUB_RUN_ATTEMPT,
      job: env.GITHUB_JOB, workflow: env.GITHUB_WORKFLOW,
      workflow_ref: env.GITHUB_WORKFLOW_REF, workflow_sha: env.GITHUB_WORKFLOW_SHA,
      runner: env.RUNNER_NAME, runner_os: env.RUNNER_OS, runner_arch: env.RUNNER_ARCH,
      run_url: (env.GITHUB_SERVER_URL + "/" + env.GITHUB_REPOSITORY + "/actions/runs/" + env.GITHUB_RUN_ID)}' \
      > "$evidence/identity.json"
    nix --version > "$evidence/nix-version.txt"
    [[ $(git rev-parse HEAD) == "${QUALIFICATION_HEAD:?}" ]] || fail 'Checkout is not the exact candidate head'
    [[ ! -s "$evidence/candidate-status.txt" ]] || fail 'Candidate source must be clean'
    [[ ${GITHUB_RUN_ATTEMPT:?} == 1 ]] || fail 'Qualification requires original run attempt 1'
    git cat-file -e "$baseline^{commit}"
    printf '%s\n' "$baseline" > "$evidence/baseline-revision.txt"
    git rev-parse "$baseline:$helper" > "$evidence/baseline-helper-blob.txt"
    git show "$baseline:$helper" > "$evidence/retained-old-helper.nix"
    (cd "$evidence"; sha256sum retained-old-helper.nix) > "$evidence/baseline-helper.sha256"
    cat > "$evidence/red.nix" <<'NIX'
let
  root = builtins.getFlake (builtins.getEnv "HARBOR_QUALIFICATION_SOURCE");
  self = root.legacy.harbor-rs;
  system = "x86_64-linux";
  pkgs = import root.inputs.nixpkgs {
    inherit system;
    overlays = [ root.inputs.rust-overlay.overlays.default ];
  };
  cross = self.lib.mkCross { inherit pkgs system; };
  mkCrossPackages = import ./retained-old-helper.nix { inherit (self.lib) mkToolchain; };
in
  (import (root.outPath + "/components/rust/tests/musl-openssl.nix") {
    inherit self pkgs system cross mkCrossPackages;
  }).packages.musl-openssl-x86_64-linux-musl
NIX
    cat > "$evidence/format.nix" <<'NIX'
let
  root = builtins.getFlake (builtins.getEnv "HARBOR_QUALIFICATION_SOURCE");
  pkgs = import root.inputs.nixpkgs {
    system = "x86_64-linux";
    overlays = [ root.inputs.rust-overlay.overlays.default ];
  };
  toolchain = root.legacy.harbor-rs.lib.mkToolchain {
    inherit pkgs;
    toolchainProfile = "nightly";
    cache.enable = false;
  };
  # The root configuration deliberately excludes test fixtures. This focused
  # treefmt configuration enrolls the regression fixture without changing that
  # project-wide exclusion or bypassing treefmt's formatter orchestration.
  formatting = root.inputs.treefmt-nix.lib.evalModule pkgs {
    projectRootFile = "flake.nix";
    programs.alejandra = {
      enable = true;
      includes = [
        "components/rust/lib/cross-packages.nix"
        "components/rust/checks.nix"
        "components/rust/tests/musl-openssl.nix"
      ];
    };
    programs.rustfmt = {
      enable = true;
      edition = "2024";
      package = toolchain.rustToolchain;
      includes = [ "components/rust/tests/fixtures/musl-openssl/src/main.rs" ];
    };
  };
in pkgs.runCommand "rust-musl-openssl-format" {
  nativeBuildInputs = [ formatting.config.build.wrapper ];
} ''
  cp -R ${root.outPath} source
  chmod -R u+w source
  cd source
  treefmt --ci
  touch "$out"
''
NIX
    ;;
  format)
    build format --impure --file "$evidence/format.nix"
    ;;
  red)
    if build red --impure --file "$evidence/red.nix"; then
      fail 'Baseline unexpectedly succeeded; RED was not reproduced'
    fi
    # All three signals must occur in one complete Cargo derivation log.
    # Download, evaluation, missing compiler, and unrelated failures cannot pass.
    reproduced=false
    for log in "$evidence"/red/nix-logs/*musl-openssl-x86_64-linux-musl*.drv.log; do
      [[ -f "$log" ]] || continue
      # A partial/failed nix-log retrieval is not complete regression evidence.
      [[ -f "$evidence/red/logs-retained.txt" ]] || continue
      grep -Fxq "/nix/store/$(basename "$log" .log)" "$evidence/red/logs-retained.txt" || continue
      if grep -Eq '(undefined reference to|undefined symbol:).*(__isoc23_[[:alnum:]_]+|__[[:alnum:]_]+_chk|__xpg_strerror_r)' "$log" \
          && grep -Eq '(libcrypto|libssl|openssl)' "$log" \
          && grep -Eq 'x86_64[^[:space:]]*-linux-musl[^[:space:]]*/bin/[^[:space:]]*(cc|gcc|ld)' "$log"; then
        printf '%s\n' "${log#"$evidence/"}" >> "$evidence/red/matching-logs.txt"
        reproduced=true
      fi
    done
    [[ $reproduced == true ]] || fail 'Baseline failed without the expected x86_64 musl OpenSSL/glibc linker signature'
    printf '%s\n' HARBOR_MUSL_OPENSSL_RED_REPRODUCED > "$evidence/red/verdict.txt"
    ;;
  green)
    build green ".#$check"
    output=$(readlink -f "$evidence/green/result")
    mkdir -p "$evidence/green/output"
    cp "$output/execution.log" "$evidence/green/output/execution.log"
    grep -Fx HARBOR_MUSL_OPENSSL_OK "$evidence/green/output/execution.log"
    nix path-info --json "$output" > "$evidence/green/check-path-info.json"
    for target in x86_64-linux-musl aarch64-linux-musl; do
      package=$(nix eval --no-update-lock-file --raw ".#$check.packages.musl-openssl-$target.outPath")
      nix path-info --json "$package" > "$evidence/green/$target-path-info.json"
      mkdir -p "$evidence/green/output/$target"
      cp "$package/bin/harbor-musl-openssl-fixture" "$evidence/green/output/$target/"
    done
    tar -C "$evidence/green/output" -czf "$evidence/green/result.tar.gz" .
    printf '%s\n' HARBOR_MUSL_OPENSSL_GREEN_VALIDATED > "$evidence/green/verdict.txt"
    ;;
  seal)
    # Result symlinks point into /nix/store; copied results and path-info are the
    # portable evidence. Hash every retained regular file, including failed logs.
    (cd "$evidence"; find . -type f ! -name evidence.sha256 -print0 | sort -z | xargs -0 sha256sum) \
      > "$evidence/evidence.sha256"
    ;;
  *) fail 'Usage: qualify-musl-openssl.sh prepare|format|red|green|seal' ;;
esac
