# mkHomebrewFormula :: { pkgs, name, version, description, homepage, license, platforms, ... }
#                   -> { formulaText; formulaPath; }
#
# Generate a Homebrew binary formula from release metadata. harbor-rs only
# emits the formula file; computing archive checksums and publishing a tap
# stay in downstream release workflows.
{packageTests}: {
  pkgs,
  name,
  version,
  description,
  homepage,
  license,
  platforms,
  dependencies ? [],
  binaries ? [name],
  caveats ? null,
  testBlock ? null,
  extraRubyBody ? "",
}: let
  inherit (pkgs) lib;

  platformKeys =
    if builtins.isAttrs platforms
    then builtins.attrNames platforms
    else [];
  allowedPlatformKeys = [
    "darwin_arm"
    "darwin_intel"
    "linux_arm"
    "linux_intel"
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
  validPlatforms = builtins.isAttrs platforms && platformKeys != [];
  invalidPlatformKeys = lib.filter (key: !(builtins.elem key allowedPlatformKeys)) platformKeys;
  validBinaries = builtins.isList binaries && binaries != [] && lib.all builtins.isString binaries;
  validDependencies = builtins.isList dependencies && lib.all builtins.isString dependencies;
  validCaveats = caveats == null || builtins.isString caveats;
  validTestBlock = testBlock == null || builtins.isString testBlock;
  validExtraRubyBody = builtins.isString extraRubyBody;

  validSha256 = sha256:
    sha256 == ":no_check" || builtins.match "[0-9a-fA-F]{64}" sha256 != null;

  validatePlatform = key: value:
    assert builtins.isAttrs value
    || throw "mkHomebrewFormula: platforms.${key} must be an attrset";
    assert value ? url
    || throw "mkHomebrewFormula: platforms.${key}.url is required";
    assert value ? sha256
    || throw "mkHomebrewFormula: platforms.${key}.sha256 is required";
    assert builtins.isString value.url
    && lib.hasPrefix "https://" value.url
    || throw "mkHomebrewFormula: platforms.${key}.url must start with https://";
    assert builtins.isString value.sha256
    && validSha256 value.sha256
    || throw "mkHomebrewFormula: platforms.${key}.sha256 must be 64 hex characters or the literal \":no_check\""; true;

  platformValidations = map (key: validatePlatform key platforms.${key}) platformKeys;

  upperFirst = part: let
    first = builtins.substring 0 1 part;
    rest = builtins.substring 1 (builtins.stringLength part - 1) part;
  in
    lib.toUpper first + rest;

  className = lib.concatStrings (map upperFirst (lib.splitString "-" name));
  rubyString = builtins.toJSON;
  indentLines = prefix: lines:
    map (line:
      if line == ""
      then ""
      else prefix + line)
    lines;
  textLines = text: lib.splitString "\n" text;
  joinBlocks = blocks: lib.concatLists (lib.intersperse [""] (lib.filter (block: block != []) blocks));

  renderSha256 = sha256:
    if sha256 == ":no_check"
    then "sha256 :no_check"
    else "sha256 ${rubyString sha256}";

  renderDownloadLines = key: [
    "url ${rubyString platforms.${key}.url}"
    (renderSha256 platforms.${key}.sha256)
  ];

  renderArchBlockLines = key: arch:
    if builtins.hasAttr key platforms
    then
      ["on_${arch} do"]
      ++ indentLines "  " (renderDownloadLines key)
      ++ ["end"]
    else [];

  renderOsBlockLines = os: armKey: intelKey: let
    archLines = joinBlocks [
      (renderArchBlockLines armKey "arm")
      (renderArchBlockLines intelKey "intel")
    ];
  in
    if archLines != []
    then ["on_${os} do"] ++ indentLines "  " archLines ++ ["end"]
    else [];

  platformLines = joinBlocks [
    (renderOsBlockLines "macos" "darwin_arm" "darwin_intel")
    (renderOsBlockLines "linux" "linux_arm" "linux_intel")
  ];

  dependencyLines = map (dep: "depends_on ${rubyString dep}") dependencies;
  installLines = map (binary: "bin.install ${rubyString binary}") binaries;

  caveatsLines =
    if caveats == null
    then []
    else
      [
        "def caveats"
        "  <<~EOS"
      ]
      ++ indentLines "  " (textLines caveats)
      ++ [
        "  EOS"
        "end"
      ];

  testBlockLines =
    if testBlock == null
    then []
    else ["test do"] ++ indentLines "  " (textLines testBlock) ++ ["end"];

  extraRubyBodyLines =
    if extraRubyBody == ""
    then []
    else textLines extraRubyBody;

  bodyBlocks = [
    dependencyLines
    (["def install"] ++ indentLines "  " installLines ++ ["end"])
    caveatsLines
    testBlockLines
    extraRubyBodyLines
  ];

  formulaLines =
    [
      "class ${className} < Formula"
      "  desc ${rubyString description}"
      "  homepage ${rubyString homepage}"
      "  version ${rubyString version}"
      "  license ${rubyString license}"
      ""
    ]
    ++ indentLines "  " platformLines
    ++ [
      ""
    ]
    ++ indentLines "  " (joinBlocks bodyBlocks)
    ++ [
      "end"
    ];

  formulaText = lib.concatStringsSep "\n" formulaLines;

  formulaPath = pkgs.runCommand "${name}.rb" {} ''
    cat > "$out" <<'FORMULA'
    ${formulaText}
    FORMULA
  '';
  artifactBuilder = packageTests.mkArtifactBuilder {
    kind = "homebrew-builder";
    packageName = name;
    inherit version;
    output = toString formulaPath;
    buildCommand = "nix build .#${name}-homebrew-formula";
    metadata = {
      inherit platforms dependencies binaries;
      helper = "mkHomebrewFormula";
      outputKind = "homebrew-formula";
    };
  };
in
  assert validName
  || throw "mkHomebrewFormula: name must match [a-z][a-z0-9-]*, got: ${name}";
  assert validVersion
  && !hasLeadingV
  || throw "mkHomebrewFormula: version must match [0-9A-Za-z.+~_-]+ without a leading v, got: ${version}";
  assert validDescription
  || throw "mkHomebrewFormula: description must be a non-empty string of 80 characters or fewer";
  assert validHomepage
  || throw "mkHomebrewFormula: homepage must start with https://";
  assert validLicense
  || throw "mkHomebrewFormula: license must be a non-empty string";
  assert validPlatforms
  || throw "mkHomebrewFormula: platforms must be a non-empty attrset";
  assert invalidPlatformKeys
  == []
  || throw "mkHomebrewFormula: unsupported platform keys: ${lib.concatStringsSep ", " invalidPlatformKeys}";
  assert lib.all (x: x) platformValidations;
  assert validDependencies
  || throw "mkHomebrewFormula: dependencies must be a list of formula name strings";
  assert validBinaries
  || throw "mkHomebrewFormula: binaries must be a non-empty list of strings";
  assert validCaveats
  || throw "mkHomebrewFormula: caveats must be null or a string";
  assert validTestBlock
  || throw "mkHomebrewFormula: testBlock must be null or a string";
  assert validExtraRubyBody
  || throw "mkHomebrewFormula: extraRubyBody must be a string"; {
    inherit formulaText formulaPath artifactBuilder;
  }
