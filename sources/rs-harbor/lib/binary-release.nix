# Reproducible release archives and locked-input consumers for static Rust
# binaries.  The producer deliberately accepts already-built derivations so a
# project can choose its own crane/cross helper while rs-harbor owns the
# archive contract shared by all consumers.
{pkgs}: let
  lib = pkgs.lib;
  inherit (lib) concatStringsSep escapeShellArg optional optionalString;
  common = import ./release-common.nix {inherit lib;};
  inherit (common) expectedMachine requireBinaries requireNonEmpty staticElfValidation deterministicTarFlags;

  mkArchive = target: spec: let
    pname = requireNonEmpty "binary release pname" spec.pname;
    version = requireNonEmpty "binary release version" spec.version;
    system = requireNonEmpty "binary release system" (spec.system or target);
    rustTarget = requireNonEmpty "binary release rustTarget" (spec.rustTarget or system);
    binaries = requireBinaries {binaries = spec.binaries;};
    binutils = spec.binutils or pkgs.binutils;
    strip = spec.strip or "${binutils}/bin/strip";
    readelf = spec.readelf or "${binutils}/bin/readelf";
    package =
      if spec ? package
      then spec.package
      else throw "rs-harbor: binary release '${target}' is missing package";
    archiveName = "${pname}-${version}-${system}-musl.tar.gz";
    manifest = builtins.toJSON {
      schemaVersion = 1;
      name = pname;
      inherit version system rustTarget binaries;
    };
    binaryArgs = concatStringsSep " " (map escapeShellArg binaries);
  in
    pkgs.runCommand "${pname}-${version}-${system}-release" {
      nativeBuildInputs = [
        pkgs.binutils
        pkgs.coreutils
        pkgs.file
        pkgs.gawk
        pkgs.gnugrep
        pkgs.gnutar
        pkgs.gzip
        pkgs.jq
      ];
    } ''
      set -euo pipefail
      stage="$TMPDIR/stage"
      mkdir -p "$stage/bin"

      for binary in ${binaryArgs}; do
        source="${package}/bin/$binary"
        test -f "$source" || {
          echo "missing release binary: $source" >&2
          exit 1
        }
        test -x "$source" || {
          echo "release binary is not executable: $source" >&2
          exit 1
        }
        install -m0755 "$source" "$stage/bin/$binary"
        # Crane may leave debug paths in the executable. Strip before
        # validation so the published binary has no Nix-store references.
        ${strip} --strip-all "$stage/bin/$binary"
        ${staticElfValidation {
        inherit readelf;
        grep = "${pkgs.gnugrep}/bin/grep";
        path = "\"$stage/bin/$binary\"";
        machine = expectedMachine {
          inherit system;
          context = "binary release";
        };
      }}
      done

      cat > "$stage/manifest.json" <<'MANIFEST'
      ${manifest}
      MANIFEST
      ${pkgs.jq}/bin/jq -e . "$stage/manifest.json" >/dev/null

      mkdir -p "$out"
      ${pkgs.gnutar}/bin/tar \
        ${deterministicTarFlags} \
        --mtime='@1' \
        -czf "$out/${archiveName}" \
        -C "$stage" .
    '';

  mkBinaryRelease = {
    pname,
    version,
    artifacts,
  }: let
    archives = lib.mapAttrs (target: spec:
      mkArchive target (spec // {inherit pname version;}))
    artifacts;
    releaseArtifacts = lib.mapAttrs (target: archive: let
      spec = artifacts.${target};
      system = spec.system or target;
      archiveName = "${pname}-${version}-${system}-musl.tar.gz";
    in
      (import ./release-artifacts.nix {inherit pkgs;}).mkReleaseArtifact {
        inherit pname version system;
        name = archiveName;
        source = archive;
        sourcePath = archiveName;
        kind = "binary-archive";
        format = "tar.gz";
        rustTarget = spec.rustTarget or system;
        validation = "static-archive";
        consumable = true;
      })
    archives;
    releaseBundle = (import ./release-artifacts.nix {inherit pkgs;}).mkReleaseBundle {
      inherit pname version;
      artifacts = releaseArtifacts;
    };
  in {
    inherit archives;
    bundle = pkgs.symlinkJoin {
      name = "${pname}-${version}-release-bundle";
      paths = builtins.attrValues archives;
    };
    inherit releaseBundle;
  };

  mkReleaseBinaryPackage = {
    pname,
    version,
    sources,
    binaries,
    runtimeInputs ? [],
    meta ? {},
  }: let
    system = pkgs.stdenv.hostPlatform.system;
    src =
      if builtins.hasAttr system sources
      then sources.${system}
      else throw "rs-harbor: no prebuilt binary release source for system '${system}'";
    expectedBinaries = requireBinaries {inherit binaries;};
    binaryArgs = concatStringsSep " " (map escapeShellArg expectedBinaries);
    expectedJson = builtins.toJSON expectedBinaries;
    expectedElfMachine = expectedMachine {
      inherit system;
      context = "binary release";
    };
  in
    pkgs.stdenvNoCC.mkDerivation {
      pname = "${pname}-prebuilt";
      inherit version meta;
      src = null;
      dontUnpack = true;
      nativeBuildInputs = optional (runtimeInputs != []) pkgs.makeWrapper;
      installPhase =
        ''
          set -euo pipefail
          manifest="${src}/manifest.json"
          test -f "$manifest" || {
            echo "prebuilt release is missing manifest.json" >&2
            exit 1
          }
          ${pkgs.jq}/bin/jq -e \
            --arg name ${escapeShellArg pname} \
            --arg version ${escapeShellArg version} \
            --arg system ${escapeShellArg system} \
            --argjson binaries '${expectedJson}' \
            '(.schemaVersion == 1) and (.name == $name) and (.version == $version) and (.system == $system) and (.binaries == $binaries)' \
            "$manifest" >/dev/null || {
              echo "prebuilt release manifest does not match ${pname}-${version}-${system}" >&2
              exit 1
            }

          mkdir -p "$out/bin"
          for binary in ${binaryArgs}; do
            source="${src}/bin/$binary"
            test -x "$source" || {
              echo "prebuilt release binary is missing or not executable: $source" >&2
              exit 1
            }
            ${staticElfValidation {
            readelf = "${pkgs.binutils}/bin/readelf";
            grep = "${pkgs.gnugrep}/bin/grep";
            path = "\"$source\"";
            machine = expectedElfMachine;
            label = "prebuilt release binary";
          }}
            install -m0755 "$source" "$out/bin/$binary"
          done
        ''
        + optionalString (runtimeInputs != []) ''
          for binary in ${binaryArgs}; do
            wrapProgram "$out/bin/$binary" \
              --prefix PATH : ${escapeShellArg (lib.makeBinPath runtimeInputs)}
          done
        '';
    };
in {
  inherit mkBinaryRelease mkReleaseBinaryPackage;
}
