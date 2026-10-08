# Portable Linux release archives backed by the pinned nix-bundle bundler.
# The bundler owns closure collection; rs-harbor owns the archive and manifest
# contract consumed by downstream flakes.
{
  pkgs,
  bundlers,
}: let
  lib = pkgs.lib;
  inherit (lib) concatStringsSep escapeShellArg;
  common = import ./release-common.nix {inherit lib;};
  inherit (common) requireBinaries requireNonEmpty;

  mkArchive = {
    pname,
    version,
    system,
    entries,
  }: let
    binaries = requireBinaries {
      context = "portable release";
      binaries = builtins.attrNames entries;
    };
    manifest = builtins.toJSON {
      schemaVersion = 2;
      name = pname;
      inherit version system binaries;
      format = "nix-bundle";
    };
    installLines = concatStringsSep "\n" (map (binary: "install -m0755 ${escapeShellArg (toString entries.${binary})} \"$out/bin/${binary}\"") binaries);
    package = pkgs.runCommand "${pname}-${version}-${system}-portable-stage" {} ''
      set -euo pipefail
      mkdir -p "$out/bin"
      ${installLines}
      cat > "$out/manifest.json" <<'MANIFEST'
      ${manifest}
      MANIFEST
    '';
    archiveName = "${pname}-${version}-${system}-nix-bundle.tar.gz";
    archive =
      pkgs.runCommand "${pname}-${version}-${system}-portable-archive" {
        nativeBuildInputs = [pkgs.coreutils pkgs.gnutar pkgs.gzip];
      } ''
        set -euo pipefail
        mkdir -p "$out"
        tar --sort=name --owner=0 --group=0 --numeric-owner --mtime='@0' \
          -czf "$out/${archiveName}" -C ${escapeShellArg (toString package)} .
      '';
  in {
    inherit archive archiveName package manifest;
  };

  mkPortableBinaryRelease = {
    pname,
    version,
    artifacts,
  }: let
    prepared = lib.mapAttrs (system: spec: let
      bundler =
        spec.bundler or (
          if builtins.hasAttr system bundlers
          then bundlers.${system}
          else throw "rs-harbor: no nix-bundle bundler for system '${system}'"
        );
      entries = lib.mapAttrs (binary: entry: let
        binaryName = requireNonEmpty "portable release binary" binary;
        package = entry.package or (throw "rs-harbor: portable entry '${binaryName}' is missing package");
        source = "${package}/bin/${binaryName}";
        wrapper =
          pkgs.runCommand "${pname}-${binaryName}-portable-entrypoint" {
            inherit version;
            pname = "${pname}-${binaryName}";
            meta.mainProgram = binaryName;
          } ''
            set -euo pipefail
            test -x ${escapeShellArg source}
            mkdir -p "$out/bin"
            ln -s ${escapeShellArg source} "$out/bin/${binaryName}"
          '';
      in
        bundler wrapper) (spec.entries or {});
    in
      mkArchive {
        inherit pname version system;
        entries = builtins.mapAttrs (_: bundle: toString bundle) entries;
      })
    artifacts;
    releaseArtifacts = lib.mapAttrs (system: result:
      (import ./release-artifacts.nix {inherit pkgs;}).mkReleaseArtifact {
        inherit pname version system;
        name = result.archiveName;
        source = result.archive;
        sourcePath = result.archiveName;
        kind = "portable-binary-archive";
        format = "tar.gz";
        validation = "portable-executable";
        consumable = true;
      })
    prepared;
    releaseBundle = (import ./release-artifacts.nix {inherit pkgs;}).mkReleaseBundle {
      inherit pname version;
      artifacts = releaseArtifacts;
    };
  in {
    archives = lib.mapAttrs (_: result: result.archive) prepared;
    inherit releaseBundle;
  };

  mkPortableReleaseBinaryPackage = {
    pname,
    version,
    sources,
    binaries,
    meta ? {},
  }: let
    system = pkgs.stdenv.hostPlatform.system;
    src =
      if builtins.hasAttr system sources
      then sources.${system}
      else throw "rs-harbor: no portable release source for system '${system}'";
    expectedBinaries = requireBinaries {
      context = "portable release";
      inherit binaries;
    };
    binaryArgs = concatStringsSep " " (map escapeShellArg expectedBinaries);
    expectedJson = builtins.toJSON expectedBinaries;
  in
    pkgs.stdenvNoCC.mkDerivation {
      pname = "${pname}-portable-prebuilt";
      inherit version meta;
      src = null;
      dontUnpack = true;
      installPhase = ''
        set -euo pipefail
        manifest="${src}/manifest.json"
        test -f "$manifest" || {
          echo "portable release is missing manifest.json" >&2
          exit 1
        }
        ${pkgs.jq}/bin/jq -e \
          --arg name ${escapeShellArg pname} \
          --arg version ${escapeShellArg version} \
          --arg system ${escapeShellArg system} \
          --argjson binaries '${expectedJson}' \
          '(.schemaVersion == 2) and (.name == $name) and (.version == $version) and (.system == $system) and (.format == "nix-bundle") and (.binaries == $binaries)' \
          "$manifest" >/dev/null || {
            echo "portable release manifest does not match ${pname}-${version}-${system}" >&2
            exit 1
          }
        mkdir -p "$out/bin"
        for binary in ${binaryArgs}; do
          source="${src}/bin/$binary"
          test -x "$source" || {
            echo "portable release binary is missing or not executable: $source" >&2
            exit 1
          }
          install -m0755 "$source" "$out/bin/$binary"
        done
      '';
    };
in {
  inherit mkPortableBinaryRelease mkPortableReleaseBinaryPackage;
}
