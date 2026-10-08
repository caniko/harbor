# mkCoprSpec :: { pkgs, name, version, summary, license, ... }
#             -> { specText; specPath; coprMakefilePath?; }
#
# Generate a Fedora RPM .spec for packaging via COPR (Fedora's COmmunity
# PRoject build service). rs-harbor only emits the spec (and optionally a
# `.copr/Makefile` for COPR's custom-build method) — publishing to COPR
# stays in CI where the credentials live.
#
# Workflow (binary-shipping default):
#   1. nix build .#my-app                    # build the binary in Nix
#   2. nix build .#copr-spec                 # generate the .spec
#   3. tar czf my-app-1.0.0.tar.gz my-app    # bundle binary as Source0
#   4. copr-cli build my-project ./result    # upload spec + tarball
#
# Example:
#   coprSpec = rs-harbor.lib.mkCoprSpec {
#     inherit pkgs;
#     name = "my-app";
#     version = "1.0.0";
#     summary = "My example application";
#     license = "MIT";
#     url = "https://example.com/my-app";
#   };
{packageTests}: {
  pkgs,
  name,
  version,
  release ? "1%{?dist}",
  summary,
  license,
  url ? null,
  sources ? [],
  buildArch ? null,
  buildRequires ? [],
  requires ? [],
  description ? null,
  prep ? null,
  build ? null,
  install ? null,
  files ? null,
  desktopFile ? null,
  icon ? null,
  appId ? null,
  changelog ? [],
  coprMakefile ? false,
  extraSections ? "",
}: let
  inherit (pkgs) lib;

  validName = builtins.match "[a-zA-Z][a-zA-Z0-9._+-]*" name;
  validVersion = builtins.match "[a-zA-Z0-9._+~]+" version;
  validAppId =
    if appId == null
    then true
    else builtins.match "[a-zA-Z][a-zA-Z0-9_]*(\\.[a-zA-Z][a-zA-Z0-9_]*)+" appId != null;
in
  assert validName
  != null
  || throw "mkCoprSpec: name must match RPM Name pattern, got: ${name}";
  assert validVersion
  != null
  || throw "mkCoprSpec: version must not contain '-' or whitespace, got: ${version}";
  assert builtins.isString summary
  && summary != ""
  || throw "mkCoprSpec: summary must be a non-empty string";
  assert builtins.isString license
  && license != ""
  || throw "mkCoprSpec: license must be a non-empty string (RPM requires License:)";
  assert validAppId
  || throw "mkCoprSpec: appId must be reverse-DNS format, got: ${toString appId}"; let
    hasDesktop = desktopFile != null;
    hasIcon = icon != null;
    effectiveAppId =
      if appId != null
      then appId
      else name;
    iconBaseName =
      if hasIcon
      then builtins.baseNameOf (builtins.toString icon)
      else null;

    indexedSources = lib.lists.imap0 (i: src: "Source${toString i}: ${src}") sources;
    sourceLines =
      if sources == []
      then ["Source0: %{name}-%{version}.tar.gz"]
      else indexedSources;

    buildRequiresLines = map (br: "BuildRequires: ${br}") buildRequires;
    requiresLines = map (r: "Requires: ${r}") requires;

    headerLines =
      [
        "Name:    ${name}"
        "Version: ${version}"
        "Release: ${release}"
        "Summary: ${summary}"
        "License: ${license}"
      ]
      ++ lib.optional (url != null) "URL:     ${url}"
      ++ lib.optional (buildArch != null) "BuildArch: ${buildArch}"
      ++ sourceLines
      ++ buildRequiresLines
      ++ requiresLines;

    defaultPrep = "%autosetup -c -T\ncp -a %{_sourcedir}/. .";
    defaultBuild = ":";

    defaultInstallLines =
      [
        "rm -rf %{buildroot}"
        "install -Dm755 %{name} %{buildroot}%{_bindir}/%{name}"
      ]
      ++ lib.optional hasDesktop
      "install -Dm644 %{name}.desktop %{buildroot}%{_datadir}/applications/${effectiveAppId}.desktop"
      ++ lib.optional hasIcon
      "install -Dm644 ${iconBaseName} %{buildroot}%{_datadir}/icons/hicolor/256x256/apps/${effectiveAppId}.png";

    defaultFilesLines =
      [
        "%{_bindir}/%{name}"
      ]
      ++ lib.optional hasDesktop "%{_datadir}/applications/${effectiveAppId}.desktop"
      ++ lib.optional hasIcon "%{_datadir}/icons/hicolor/256x256/apps/${effectiveAppId}.png";

    effectivePrep =
      if prep != null
      then prep
      else defaultPrep;
    effectiveBuild =
      if build != null
      then build
      else defaultBuild;
    effectiveInstall =
      if install != null
      then install
      else lib.concatStringsSep "\n" defaultInstallLines;
    effectiveFiles =
      if files != null
      then files
      else lib.concatStringsSep "\n" defaultFilesLines;
    effectiveDescription =
      if description != null
      then description
      else summary;

    validChangelogEntry = e:
      builtins.isAttrs e
      && e ? date
      && e ? author
      && e ? version
      && e ? entries
      && builtins.isList e.entries;

    formatChangelogEntry = e:
      assert validChangelogEntry e
      || throw "mkCoprSpec: each changelog entry must be { date; author; version; entries = [..]; }";
        "* ${e.date} ${e.author} - ${e.version}\n"
        + lib.concatMapStringsSep "\n" (line: "- ${line}") e.entries;

    changelogText =
      if changelog == []
      then ""
      else lib.concatMapStringsSep "\n\n" formatChangelogEntry changelog;

    specText =
      lib.concatStringsSep "\n" headerLines
      + "\n\n"
      + "%description\n${effectiveDescription}\n\n"
      + "%prep\n${effectivePrep}\n\n"
      + "%build\n${effectiveBuild}\n\n"
      + "%install\n${effectiveInstall}\n\n"
      + "%files\n${effectiveFiles}\n"
      + lib.optionalString (extraSections != "") "\n${extraSections}\n"
      + lib.optionalString (changelogText != "") "\n%changelog\n${changelogText}\n";

    specPath = pkgs.writeText "${name}.spec" specText;

    coprMakefileText = lib.concatStringsSep "\n" [
      "# Custom-build entrypoint for COPR. COPR clones the repo and runs"
      "# `make srpm` with $outdir pointing at where the SRPM should land."
      "# Reference: https://docs.pagure.org/copr.copr/user_documentation.html"
      "SPEC := ${name}.spec"
      ""
      ".PHONY: srpm"
      ""
      "srpm:"
      "\tdnf -y install rpm-build"
      "\trpmbuild -bs \\"
      "\t\t--define \"_sourcedir $(CURDIR)\" \\"
      "\t\t--define \"_srcrpmdir $(outdir)\" \\"
      "\t\t$(SPEC)"
    ];

    coprMakefilePath =
      if coprMakefile
      then pkgs.writeText "${name}-copr-Makefile" coprMakefileText
      else null;
    artifactBuilder = packageTests.mkArtifactBuilder {
      kind = "copr-rpm-builder";
      packageName = name;
      inherit version;
      output = toString specPath;
      buildCommand = "nix build .#${name}-copr-spec";
      metadata = {
        inherit release license sources buildArch buildRequires requires coprMakefile;
        helper = "mkCoprSpec";
        outputKind = "rpm-spec";
      };
    };
  in
    {
      inherit specText specPath artifactBuilder;
    }
    // lib.optionalAttrs coprMakefile {inherit coprMakefilePath;}
