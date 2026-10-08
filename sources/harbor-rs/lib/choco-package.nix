{packageTests}: {
  pkgs,
  id,
  version,
  description,
  summary ? null,
  homepage,
  license,
  licenseUrl,
  authors,
  owners ? authors,
  projectSource ? homepage,
  architectures,
  binaries ? [id],
  releaseNotesUrl ? null,
  iconUrl ? null,
  tags ? [],
  extraNuspecFields ? {},
}: let
  inherit (pkgs) lib;

  architectureKeys =
    if builtins.isAttrs architectures
    then builtins.attrNames architectures
    else [];
  allowedArchitectureKeys = [
    "x64"
    "x86"
    "arm64"
  ];

  validId = builtins.isString id && builtins.match "[a-z][a-z0-9-]*" id != null;
  # Chocolatey prefers SemVer-clean versions; keep validation broad enough for
  # prerelease staging metadata, but never add a leading v.
  validVersion = builtins.isString version && builtins.match "[0-9A-Za-z.+~_-]+" version != null;
  hasLeadingV = builtins.isString version && builtins.match "v.*" version != null;
  validDescription =
    builtins.isString description
    && description != ""
    && builtins.stringLength description <= 4000;
  validSummary = summary == null || (builtins.isString summary && summary != "" && builtins.stringLength summary <= 80);
  validHomepage = builtins.isString homepage && lib.hasPrefix "https://" homepage;
  validLicense = builtins.isString license && license != "";
  validLicenseUrl = builtins.isString licenseUrl && lib.hasPrefix "https://" licenseUrl;
  validProjectSource = builtins.isString projectSource && lib.hasPrefix "https://" projectSource;
  validReleaseNotesUrl = releaseNotesUrl == null || (builtins.isString releaseNotesUrl && lib.hasPrefix "https://" releaseNotesUrl);
  validIconUrl = iconUrl == null || (builtins.isString iconUrl && lib.hasPrefix "https://" iconUrl);
  validArchitectures = builtins.isAttrs architectures && architectureKeys != [];
  invalidArchitectureKeys = lib.filter (key: !(builtins.elem key allowedArchitectureKeys)) architectureKeys;
  validBinaries = builtins.isList binaries && binaries != [] && lib.all (binary: builtins.isString binary && binary != "") binaries;
  validTags = builtins.isList tags && lib.all (tag: builtins.isString tag && tag != "" && !(lib.hasInfix " " tag)) tags;
  validAuthors = builtins.isList authors && authors != [] && lib.all (author: builtins.isString author && author != "") authors;
  validOwners = builtins.isList owners && owners != [] && lib.all (owner: builtins.isString owner && owner != "") owners;
  validExtraNuspecFields = builtins.isAttrs extraNuspecFields;

  validSha256 = sha256:
    sha256 == ":no_check" || builtins.match "[0-9a-fA-F]{64}" sha256 != null;

  validateArchitecture = key: value:
    assert builtins.isAttrs value
    || throw "mkChocoPackage: architectures.${key} must be an attrset";
    assert value ? url
    || throw "mkChocoPackage: architectures.${key}.url is required";
    assert value ? sha256
    || throw "mkChocoPackage: architectures.${key}.sha256 is required";
    assert builtins.isString value.url
    && lib.hasPrefix "https://" value.url
    || throw "mkChocoPackage: architectures.${key}.url must start with https://";
    assert builtins.isString value.sha256
    && validSha256 value.sha256
    || throw "mkChocoPackage: architectures.${key}.sha256 must be 64 hex characters or the literal \":no_check\""; true;

  architectureValidations = map (key: validateArchitecture key architectures.${key}) architectureKeys;

  validateExtraField = key: let
    value = extraNuspecFields.${key};
  in
    assert builtins.match "[A-Za-z_][A-Za-z0-9_.-]*" key
    != null
    || throw "mkChocoPackage: extraNuspecFields.${key} must be a valid XML element name";
    assert builtins.isString value
    || builtins.isInt value
    || builtins.isFloat value
    || builtins.isBool value
    || throw "mkChocoPackage: extraNuspecFields.${key} must be a string, int, float, or bool"; true;

  extraFieldValidations = map validateExtraField (builtins.attrNames extraNuspecFields);

  effectiveSummary =
    if summary != null
    then summary
    else if builtins.stringLength description <= 80
    then description
    else null;

  escapeXml = value:
    builtins.replaceStrings
    ["&" "<" ">" "\"" "'"]
    ["&amp;" "&lt;" "&gt;" "&quot;" "&apos;"]
    value;

  escapePowerShellSingleQuoted = value:
    builtins.replaceStrings ["'"] ["''"] value;

  extraFieldValueToString = value:
    if builtins.isBool value
    then
      if value
      then "true"
      else "false"
    else toString value;

  xmlField = name: value: "    <${name}>${escapeXml value}</${name}>";

  metadataFields =
    [
      (xmlField "id" id)
      (xmlField "version" version)
      (xmlField "title" id)
      (xmlField "authors" (lib.concatStringsSep ", " authors))
      (xmlField "owners" (lib.concatStringsSep ", " owners))
      "    <license type=\"expression\">${escapeXml license}</license>"
      (xmlField "licenseUrl" licenseUrl)
      (xmlField "projectUrl" homepage)
      (xmlField "projectSourceUrl" projectSource)
    ]
    ++ lib.optional (iconUrl != null) (xmlField "iconUrl" iconUrl)
    ++ lib.optional (tags != []) (xmlField "tags" (lib.concatStringsSep " " tags))
    ++ lib.optional (effectiveSummary != null) (xmlField "summary" effectiveSummary)
    ++ [
      (xmlField "description" description)
    ]
    ++ lib.optional (releaseNotesUrl != null) (xmlField "releaseNotes" releaseNotesUrl)
    ++ map
    (key: xmlField key (extraFieldValueToString extraNuspecFields.${key}))
    (builtins.sort builtins.lessThan (builtins.attrNames extraNuspecFields))
    ++ [
      "    <requireLicenseAcceptance>false</requireLicenseAcceptance>"
    ];

  nuspecText = lib.concatStringsSep "\n" (
    [
      "<?xml version=\"1.0\" encoding=\"utf-8\"?>"
      "<package xmlns=\"http://schemas.microsoft.com/packaging/2015/06/nuspec.xsd\">"
      "  <metadata>"
    ]
    ++ metadataFields
    ++ [
      "  </metadata>"
      "  <files>"
      "    <file src=\"tools\\**\" target=\"tools\" />"
      "  </files>"
      "</package>"
    ]
  );

  renderPackageArg = key: value: "  ${key} = '${escapePowerShellSingleQuoted value}'";
  archArgNames = {
    x86 = {
      url = "url";
      checksum = "checksum";
      checksumType = "checksumType";
    };
    x64 = {
      url = "url64bit";
      checksum = "checksum64";
      checksumType = "checksumType64";
    };
    arm64 = {
      url = "urlArm64";
      checksum = "checksumArm64";
      checksumType = "checksumTypeArm64";
    };
  };

  renderArchPackageArgs = key: let
    arch = architectures.${key};
    argNames = archArgNames.${key};
  in
    [
      (renderPackageArg argNames.url arch.url)
    ]
    ++ lib.optional (arch.sha256 != ":no_check") (renderPackageArg argNames.checksum arch.sha256)
    ++ lib.optional (arch.sha256 != ":no_check") (renderPackageArg argNames.checksumType "sha256");

  packageArgLines =
    [
      "  packageName = '${escapePowerShellSingleQuoted id}'"
      "  unzipLocation = $toolsDir"
    ]
    ++ lib.concatMap renderArchPackageArgs (builtins.sort builtins.lessThan architectureKeys);

  # Chocolatey accepts omitted checksum keys here, which lets callers use
  # :no_check placeholders before release archives exist. Public external-feed
  # packages should still provide real sha256 values for moderation.
  installScriptText = lib.concatStringsSep "\n" (
    [
      "$ErrorActionPreference = 'Stop'"
      "$toolsDir   = \"$(Split-Path -parent $MyInvocation.MyCommand.Definition)\""
      "$packageArgs = @{"
    ]
    ++ packageArgLines
    ++ [
      "}"
      "Install-ChocolateyZipPackage @packageArgs"
    ]
  );

  nuspecPath = pkgs.writeText "${id}.nuspec" nuspecText;
  installScriptPath = pkgs.writeText "chocolateyInstall.ps1" installScriptText;

  packageDir = pkgs.runCommand "${id}-choco-package" {} ''
    mkdir -p "$out/tools"
    cp ${nuspecPath} "$out/${id}.nuspec"
    cp ${installScriptPath} "$out/tools/chocolateyInstall.ps1"
  '';

  nupkgPath =
    pkgs.runCommand "${id}.${version}.nupkg" {
      nativeBuildInputs = [pkgs.zip];
    } ''
      tmp="$TMPDIR/package"
      cp -R ${packageDir} "$tmp"
      chmod -R u+w "$tmp"
      cd "$tmp"
      zip -qr "$out" .
    '';

  artifactBuilder = packageTests.mkArtifactBuilder {
    kind = "chocolatey-builder";
    packageName = id;
    inherit version;
    output = toString nupkgPath;
    buildCommand = "nix build .#${id}-chocolatey";
    metadata = {
      inherit binaries;
      packageDir = toString packageDir;
      nuspecPath = toString nuspecPath;
      installScriptPath = toString installScriptPath;
      nupkgName = "${id}.${version}.nupkg";
    };
  };
in
  assert validId
  || throw "mkChocoPackage: id must match [a-z][a-z0-9-]*, got: ${id}";
  assert validVersion
  && !hasLeadingV
  || throw "mkChocoPackage: version must match [0-9A-Za-z.+~_-]+ without a leading v, got: ${version}";
  assert validDescription
  || throw "mkChocoPackage: description must be a non-empty string of 4000 characters or fewer";
  assert validSummary
  || throw "mkChocoPackage: summary must be null or a non-empty string of 80 characters or fewer";
  assert validHomepage
  || throw "mkChocoPackage: homepage must start with https://";
  assert validLicense
  || throw "mkChocoPackage: license must be a non-empty string";
  assert validLicenseUrl
  || throw "mkChocoPackage: licenseUrl must start with https://";
  assert validProjectSource
  || throw "mkChocoPackage: projectSource must start with https://";
  assert validReleaseNotesUrl
  || throw "mkChocoPackage: releaseNotesUrl must be null or start with https://";
  assert validIconUrl
  || throw "mkChocoPackage: iconUrl must be null or start with https://";
  assert validAuthors
  || throw "mkChocoPackage: authors must be a non-empty list of non-empty strings";
  assert validOwners
  || throw "mkChocoPackage: owners must be a non-empty list of non-empty strings";
  assert validArchitectures
  || throw "mkChocoPackage: architectures must be a non-empty attrset";
  assert invalidArchitectureKeys
  == []
  || throw "mkChocoPackage: unsupported architecture keys: ${lib.concatStringsSep ", " invalidArchitectureKeys}";
  assert lib.all (x: x) architectureValidations;
  assert validBinaries
  || throw "mkChocoPackage: binaries must be a non-empty list of non-empty strings";
  assert validTags
  || throw "mkChocoPackage: tags must be a list of non-empty strings without spaces";
  assert validExtraNuspecFields
  || throw "mkChocoPackage: extraNuspecFields must be an attrset";
  assert lib.all (x: x) extraFieldValidations; {
    inherit id version nuspecText installScriptText packageDir nupkgPath artifactBuilder;
  }
