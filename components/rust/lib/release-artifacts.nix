# Generic release artifact constructors.
#
# The artifact layer deliberately does not know about Codeberg or Simit.  It
# produces deterministic, flat Nix outputs plus a small passthru descriptor;
# Simit consumes the conventional release-bundle output in its Forgejo job.
{
  pkgs,
  timezone ? throw "harbor-rs: archive builders require harbor-meta.lib.timezone",
}: let
  lib = pkgs.lib;
  inherit (lib) concatStringsSep escapeShellArg optionalString;
  common = import ./release-common.nix {inherit lib;};
  inherit (common) expectedMachine require requireString staticElfValidation deterministicTarFlags;

  descriptor = {
    pname,
    version,
    name,
    kind,
    format,
    system ? null,
    rustTarget ? null,
    validation ? "none",
    consumable ? false,
    files ? [name],
  }:
    assert lib.assertMsg (builtins.isString pname && pname != "") "harbor-rs: release pname must be a non-empty string";
    assert lib.assertMsg (builtins.isString version && version != "") "harbor-rs: release version must be a non-empty string"; {
      schemaVersion = 2;
      inherit pname version name kind format system rustTarget validation consumable files;
    };

  artifactPassthru = meta: {
    passthru.rsHarborReleaseArtifact = meta;
  };

  mkFile = {
    pname,
    version,
    name,
    source,
    sourcePath ? null,
    kind ? "opaque",
    format ? "raw",
    system ? null,
    rustTarget ? null,
    validation ? "none",
    consumable ? false,
    executable ? false,
  }: let
    fileName = requireString "artifact name" name;
    sourceValue = require "artifact source" source;
    meta = descriptor {
      inherit pname version;
      name = fileName;
      inherit kind format system rustTarget validation consumable;
    };
    sourceArg =
      if sourcePath == null
      then "${sourceValue}"
      else "${sourceValue}/${sourcePath}";
  in
    pkgs.runCommand "${pname}-${version}-${fileName}-release-artifact" (artifactPassthru meta) ''
      set -euo pipefail
      mkdir -p "$out"
      test -f ${escapeShellArg sourceArg}
      install -m${
        if executable
        then "0755"
        else "0644"
      } ${escapeShellArg sourceArg} "$out/${fileName}"
    '';

  mkArchive = {
    pname,
    version,
    name,
    package,
    entries,
    format ? "tar.gz",
    kind ? "binary",
    system ? null,
    rustTarget ? null,
    validation ? "none",
    consumable ? false,
    archiveTimezone ? "UTC",
  }: let
    archiveName = requireString "archive name" name;
    timezoneEnv = timezone.mkEnvironment {
      inherit pkgs;
      timeZone = archiveTimezone;
    };
    packageValue = require "archive package" package;
    entryNames = builtins.attrNames entries;
    entryArgs = concatStringsSep " " (map escapeShellArg entryNames);
    entrySources = common.stageFiles {
      root = "$stage";
      files = lib.mapAttrs (_: entry: let
        spec =
          if builtins.isString entry
          then {
            source = entry;
            mode = "0755";
          }
          else entry;
        source = spec.source;
      in
        spec
        // {
          source =
            if builtins.isString source && !(lib.hasPrefix "/" source)
            then "${packageValue}/${source}"
            else source;
        })
      entries;
    };
    meta = descriptor {
      inherit pname version;
      name = archiveName;
      files = [archiveName];
      inherit kind format system rustTarget validation consumable;
    };
    validationScript =
      if validation == "static-elf"
      then
        assert lib.assertMsg (system != null) "harbor-rs: static-elf artifacts require system"; ''
          for binary in ${entryArgs}; do
            ${staticElfValidation {
            readelf = "readelf";
            grep = "grep";
            path = "\"$stage/$binary\"";
            machine = expectedMachine {inherit system;};
          }}
          done
        ''
      else "";
    archiveCommand =
      if format == "tar.gz"
      then ''tar ${deterministicTarFlags} --mtime='@0' -czf "$out/${archiveName}" -C "$stage" .''
      else if format == "zip"
      then ''cd "$stage" && find . -type f -print | LC_ALL=C sort | zip -X -q "$out/${archiveName}" -@''
      else throw "harbor-rs: unsupported release archive format '${format}'";
  in
    pkgs.runCommand "${pname}-${version}-${archiveName}-release-artifact" ((artifactPassthru meta)
      // timezoneEnv.env
      // {
        nativeBuildInputs = [pkgs.binutils pkgs.coreutils pkgs.findutils pkgs.gnugrep pkgs.gnutar pkgs.gzip pkgs.zip];
      }) ''
      set -euo pipefail
      stage="$TMPDIR/stage"
      ${timezoneEnv.validationScript}
      umask 022
      mkdir -p "$stage"
      ${entrySources}
      ${validationScript}
      # ZIP cannot represent the Unix epoch; normalize all formats to its
      # minimum timestamp before archiving. Tar still uses --mtime=@0.
      find "$stage" -exec touch -h -t 198001010000.00 {} +
      mkdir -p "$out"
      ${archiveCommand}
    '';

  mkReleaseBundle = {
    pname,
    version,
    artifacts,
  }: let
    artifactValues = builtins.attrValues artifacts;
    descriptors = map (artifact:
      if builtins.isAttrs artifact && artifact ? rsHarborReleaseArtifact
      then artifact.rsHarborReleaseArtifact
      else {
        schemaVersion = 2;
        name = builtins.baseNameOf (toString artifact);
        kind = "opaque";
        format = "directory";
        system = null;
        rustTarget = null;
        validation = "none";
        consumable = false;
        files = [];
      })
    artifactValues;
    descriptorNames = map (artifact: artifact.name) descriptors;
    manifest = builtins.toJSON {
      schemaVersion = 2;
      inherit pname version;
      artifacts = descriptors;
    };
    artifactArgs = concatStringsSep " " (map (artifact: escapeShellArg (toString artifact)) artifactValues);
  in
    assert lib.assertMsg
    (builtins.length descriptorNames == builtins.length (lib.unique descriptorNames))
    "harbor-rs: release bundle artifact names must be unique";
      pkgs.runCommand "${pname}-${version}-release-bundle" {
        nativeBuildInputs = [pkgs.coreutils pkgs.findutils pkgs.gnugrep pkgs.jq];
        passthru.rsHarborReleaseBundle = true;
      } ''
        set -euo pipefail
        mkdir -p "$out"
        for artifact in ${artifactArgs}; do
          while IFS= read -r file; do
            name="$(basename "$file")"
            test ! -e "$out/$name" || {
              echo "harbor-rs: release bundle asset collision: $name" >&2
              exit 1
            }
            cp -L "$file" "$out/$name"
          done < <(find -L "$artifact" -mindepth 1 -maxdepth 1 -type f -print | LC_ALL=C sort)
        done
        cat > "$out/${pname}-${version}-release-manifest.json" <<'MANIFEST'
        ${manifest}
        MANIFEST
        ${pkgs.jq}/bin/jq -e --arg name ${escapeShellArg pname} --arg version ${escapeShellArg version} \
          '(.schemaVersion == 2) and (.pname == $name) and (.version == $version) and (.artifacts | length > 0)' \
          "$out/${pname}-${version}-release-manifest.json" >/dev/null
      '';
in {
  inherit mkFile mkArchive mkReleaseBundle;
  mkReleaseArtifact = mkFile;
  mkReleaseArchive = mkArchive;
}
