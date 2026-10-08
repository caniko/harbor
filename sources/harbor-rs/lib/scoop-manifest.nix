#
# mkScoopManifest :: { pkgs, name, version, description, homepage, license, architectures, ... }
#                 -> { manifestText; manifestPath; }
#
# Generate a Scoop manifest from release metadata. harbor-rs only emits the
# manifest file; checksum computation and bucket publishing stay in downstream
# release workflows.
{packageTests}: {
  pkgs,
  name,
  version,
  description,
  homepage,
  license,
  architectures,
  binaries ? [name],
  persist ? [],
  checkver ? null,
  autoupdate ? null,
  extraFields ? {},
}: let
  inherit (pkgs) lib;

  architectureKeys =
    if builtins.isAttrs architectures
    then builtins.attrNames architectures
    else [];
  allowedArchitectureKeys = [
    "64bit"
    "32bit"
    "arm64"
  ];

  validName = builtins.isString name && builtins.match "[a-z][a-z0-9-]*" name != null;
  validVersion = builtins.isString version && builtins.match "[0-9A-Za-z.+~_-]+" version != null;
  hasLeadingV = builtins.isString version && builtins.match "v.*" version != null;
  validDescription =
    builtins.isString description
    && description != ""
    && builtins.stringLength description <= 80;
  validHomepage = builtins.isString homepage && lib.hasPrefix "https://" homepage;
  validLicense = builtins.isString license && license != "";
  validArchitectures = builtins.isAttrs architectures && architectureKeys != [];
  invalidArchitectureKeys = lib.filter (key: !(builtins.elem key allowedArchitectureKeys)) architectureKeys;
  validBinaries = builtins.isList binaries && binaries != [] && lib.all (binary: builtins.isString binary && binary != "") binaries;
  validPersist = builtins.isList persist && lib.all (entry: builtins.isString entry && entry != "") persist;
  validCheckver = checkver == null || builtins.isAttrs checkver;
  validAutoupdate = autoupdate == null || builtins.isAttrs autoupdate;
  validExtraFields = builtins.isAttrs extraFields;

  validSha256 = sha256:
    sha256 == ":no_check" || builtins.match "[0-9a-fA-F]{64}" sha256 != null;

  validateArchitecture = key: value:
    assert builtins.isAttrs value
    || throw "mkScoopManifest: architectures.${key} must be an attrset";
    assert value ? url
    || throw "mkScoopManifest: architectures.${key}.url is required";
    assert value ? sha256
    || throw "mkScoopManifest: architectures.${key}.sha256 is required";
    assert builtins.isString value.url
    && lib.hasPrefix "https://" value.url
    || throw "mkScoopManifest: architectures.${key}.url must start with https://";
    assert builtins.isString value.sha256
    && validSha256 value.sha256
    || throw "mkScoopManifest: architectures.${key}.sha256 must be 64 hex characters or the literal \":no_check\""; true;

  architectureValidations = map (key: validateArchitecture key architectures.${key}) architectureKeys;

  renderArchitecture = key: let
    value = architectures.${key};
  in
    {
      url = value.url;
    }
    // lib.optionalAttrs (value.sha256 != ":no_check") {
      hash = "sha256:${value.sha256}";
    };

  hasNoCheckPlaceholder = lib.any (key: architectures.${key}.sha256 == ":no_check") architectureKeys;

  # Scoop has no checksum skip sentinel. When downstream metadata still carries
  # :no_check, omit the per-arch hash and emit a top-level _comment so Phase 02
  # can preserve the exact same placeholder semantics.
  manifest =
    {
      version = version;
      description = description;
      homepage = homepage;
      license = license;
      architecture = builtins.listToAttrs (map (key: {
          name = key;
          value = renderArchitecture key;
        })
        architectureKeys);
      bin = binaries;
    }
    // lib.optionalAttrs hasNoCheckPlaceholder {
      _comment = "placeholder: hash omitted for one or more architectures because sha256 is :no_check";
    }
    // lib.optionalAttrs (persist != []) {
      persist = persist;
    }
    // lib.optionalAttrs (checkver != null) {
      checkver = checkver;
    }
    // lib.optionalAttrs (autoupdate != null) {
      autoupdate = autoupdate;
    }
    // extraFields;

  manifestText = builtins.toJSON manifest;
  manifestSource = pkgs.writeText "${name}.json.raw" manifestText;

  manifestPath =
    pkgs.runCommand "${name}.json" {
      nativeBuildInputs = [pkgs.jq];
    } ''
      jq . ${manifestSource} > "$out"
    '';
  artifactBuilder = packageTests.mkArtifactBuilder {
    kind = "scoop-builder";
    packageName = name;
    inherit version;
    output = toString manifestPath;
    buildCommand = "nix build .#${name}-scoop-manifest";
    metadata = {
      inherit architectures binaries persist;
      helper = "mkScoopManifest";
      outputKind = "scoop-manifest";
    };
  };
in
  assert validName
  || throw "mkScoopManifest: name must match [a-z][a-z0-9-]*, got: ${name}";
  assert validVersion
  && !hasLeadingV
  || throw "mkScoopManifest: version must match [0-9A-Za-z.+~_-]+ without a leading v, got: ${version}";
  assert validDescription
  || throw "mkScoopManifest: description must be a non-empty string of 80 characters or fewer";
  assert validHomepage
  || throw "mkScoopManifest: homepage must start with https://";
  assert validLicense
  || throw "mkScoopManifest: license must be a non-empty string";
  assert validArchitectures
  || throw "mkScoopManifest: architectures must be a non-empty attrset";
  assert invalidArchitectureKeys
  == []
  || throw "mkScoopManifest: unsupported architecture keys: ${lib.concatStringsSep ", " invalidArchitectureKeys}";
  assert lib.all (x: x) architectureValidations;
  assert validBinaries
  || throw "mkScoopManifest: binaries must be a non-empty list of non-empty strings";
  assert validPersist
  || throw "mkScoopManifest: persist must be a list of non-empty strings";
  assert validCheckver
  || throw "mkScoopManifest: checkver must be null or an attrset";
  assert validAutoupdate
  || throw "mkScoopManifest: autoupdate must be null or an attrset";
  assert validExtraFields
  || throw "mkScoopManifest: extraFields must be an attrset"; {
    inherit manifestText manifestPath artifactBuilder;
  }
