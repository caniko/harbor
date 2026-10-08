{
  pkgs,
  lib,
  sourceIdentity,
  cargoLock,
  flakeLock,
  rustToolchain,
  androidSdk,
  rustTarget,
  arch,
  mode,
  cargoFeatures,
  hermetic,
}: let
  cargoLockDigest = builtins.hashFile "sha256" cargoLock;
  flakeLockDigest = builtins.hashFile "sha256" flakeLock;
  toolchainDigest = builtins.hashString "sha256" (toString rustToolchain);
  featureSet = lib.concatStringsSep "," cargoFeatures;
in ''
  apk="$(${pkgs.findutils}/bin/find "$out" -maxdepth 1 -type f -name '*.apk' -print -quit)"
  if [ -z "$apk" ]; then
    echo "harbor-android: package identity requires exactly one APK" >&2
    exit 1
  fi
  apk_count="$(${pkgs.findutils}/bin/find "$out" -maxdepth 1 -type f -name '*.apk' -printf . | ${pkgs.coreutils}/bin/wc -c)"
  if [ "$apk_count" -ne 1 ]; then
    echo "harbor-android: package identity found multiple APKs" >&2
    exit 1
  fi

  apk_name="$(${pkgs.coreutils}/bin/basename "$apk")"
  package_digest="$(${pkgs.coreutils}/bin/sha256sum "$apk")"
  package_digest="''${package_digest%% *}"
  package_size="$(${pkgs.coreutils}/bin/stat --format=%s "$apk")"
  rustc_version="$(${rustToolchain}/bin/rustc --version)"
  cargo_version="$(${rustToolchain}/bin/cargo --version)"
  unsigned_base="$TMPDIR/android-unsigned-manifest.json"

  ${pkgs.jq}/bin/jq -n \
    --arg commit ${lib.escapeShellArg sourceIdentity.commit} \
    --arg workspace_digest ${lib.escapeShellArg sourceIdentity.workspaceDigest} \
    --arg target ${lib.escapeShellArg rustTarget} \
    --arg profile ${lib.escapeShellArg mode} \
    --arg features ${lib.escapeShellArg featureSet} \
    --arg rustc "$rustc_version" \
    --arg cargo "$cargo_version" \
    --arg flake_lock_digest ${lib.escapeShellArg flakeLockDigest} \
    --arg toolchain_digest ${lib.escapeShellArg toolchainDigest} \
    --arg sdk ${lib.escapeShellArg (toString androidSdk)} \
    --arg cargo_lock_digest ${lib.escapeShellArg cargoLockDigest} \
    --arg path "$apk_name" \
    --arg package_digest "$package_digest" \
    --arg arch ${lib.escapeShellArg arch} \
    --argjson size "$package_size" \
    --argjson dirty ${
    if sourceIdentity.dirty
    then "true"
    else "false"
  } \
    --argjson hermetic ${
    if hermetic
    then "true"
    else "false"
  } \
    '{
      schema_version: 1,
      source: {
        commit: $commit,
        workspace_digest: $workspace_digest,
        dirty: $dirty
      },
      environment: {
        target: $target,
        profile: $profile,
        features: $features,
        rustc: $rustc,
        cargo: $cargo,
        flake_lock_digest: $flake_lock_digest,
        toolchain_digest: $toolchain_digest,
        sdk: $sdk,
        hermetic: $hermetic
      },
      platform: "android-apk",
      feature_set: $features,
      cargo_lock_digest: $cargo_lock_digest,
      entries: [{
        path: $path,
        digest: $package_digest,
        size: $size,
        kind: "file",
        executable: false,
        symlink_target: null,
        arch: $arch,
        role: "Package"
      }]
    }' > "$unsigned_base"

  unsigned_digest="$(${pkgs.jq}/bin/jq -cSj . "$unsigned_base" | ${pkgs.coreutils}/bin/sha256sum)"
  unsigned_digest="''${unsigned_digest%% *}"
  ${pkgs.jq}/bin/jq --arg digest "$unsigned_digest" '. + {digest: $digest}' \
    "$unsigned_base" > "$out/unsigned-manifest.json"

  ${pkgs.jq}/bin/jq -n \
    --arg unsigned_digest "$unsigned_digest" \
    --arg package_digest "$package_digest" \
    '{
      platform: "android-apk",
      unsigned_digest: $unsigned_digest,
      package_digest: $package_digest,
      signed: false
    }' > "$out/distribution-manifest.json"
''
