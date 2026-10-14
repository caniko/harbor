# Install publisher-owned static Linux archives without requiring a Harbor
# manifest or rewriting the executable. Fetching and provenance are caller-owned.
{pkgs}: {
  pname,
  version,
  sources,
  binaries,
  targetSystem ? pkgs.stdenv.hostPlatform.system,
  requireExecutable ? true,
  meta ? {},
}: let
  inherit (pkgs) lib;
  inherit (lib) escapeShellArg;
  common = import ./release-common.nix {inherit lib;};
  machine = common.expectedMachine {
    system = targetSystem;
    context = "upstream binary";
  };
  validRelativePath = path:
    builtins.isString path
    && path != ""
    && !(lib.hasPrefix "/" path)
    && !(lib.hasInfix "\n" path)
    && !(lib.hasInfix "\r" path)
    && lib.all (part: part != "" && part != "." && part != "..") (lib.splitString "/" path);
  names = builtins.attrNames binaries;
  validate = name:
    assert lib.assertMsg (builtins.match "[a-zA-Z0-9][a-zA-Z0-9._+-]*" name != null)
    "harbor-rs: invalid upstream binary destination '${name}'";
    assert lib.assertMsg (validRelativePath binaries.${name})
    "harbor-rs: invalid upstream binary source for '${name}'"; name;
  validated = map validate names;
  src = sources.${targetSystem} or (throw "harbor-rs: no upstream binary source for system '${targetSystem}'");
in
  assert lib.assertMsg (builtins.isAttrs binaries && names != [])
  "harbor-rs: upstream binaries must be a non-empty destination-to-source mapping";
  assert lib.assertMsg (builtins.isBool requireExecutable)
  "harbor-rs: requireExecutable must be a boolean";
    builtins.deepSeq validated (builtins.seq machine (builtins.seq src (pkgs.stdenvNoCC.mkDerivation {
      inherit pname version src;
      # Paths in binaries are relative to the archive root, including any
      # publisher-owned enclosing directory. Never guess or flatten that layout.
      sourceRoot = ".";
      unpackPhase = ''
        runHook preUnpack
        if test -d "$src"; then
          # Locked tarball inputs are already unpacked; copy their contents,
          # including dotfiles and symlinks, without adding a store-name prefix.
          cp -R -- "$src"/. .
        else
          unpackFile "$src"
        fi
        runHook postUnpack
      '';
      nativeBuildInputs = [pkgs.xz];
      dontConfigure = true;
      dontBuild = true;
      dontFixup = true;
      strictDeps = true;
      installPhase = ''
        runHook preInstall
        set -euo pipefail
        mkdir -p "$out/bin"
        ${lib.concatMapStringsSep "\n" (name: let
            relative = binaries.${name};
          in ''
            source=${escapeShellArg "./${relative}"}
            test -f "$source" ${lib.optionalString requireExecutable ''&& test -x "$source"''} || {
              echo "upstream binary is missing, not regular, or not executable: $source" >&2
              exit 1
            }
            # Reject symlinks in every component, including an enclosing directory.
            IFS=/ read -r -a components <<< "$source"
            current=.
            for component in "''${components[@]}"; do
              current="$current/$component"
              if test -L "$current"; then
                echo "upstream binary source contains a symlink: $source" >&2
                exit 1
              fi
            done
            ${common.staticElfValidation {
              readelf = "${pkgs.binutils}/bin/readelf";
              grep = "${pkgs.gnugrep}/bin/grep";
              path = "\"$source\"";
              inherit machine;
              label = "upstream binary";
            }}
            header=$(${pkgs.binutils}/bin/readelf -h "$source")
            type=$(printf '%s\n' "$header" | ${pkgs.gawk}/bin/awk '/^[[:space:]]*Type:/ { print $2 }')
            case "$type" in
              EXEC|DYN) ;;
              *) echo "upstream binary has a non-executable ELF type: $source" >&2; exit 1 ;;
            esac
            entry=$(printf '%s\n' "$header" | ${pkgs.gawk}/bin/awk '/Entry point address:/ { print $4 }')
            # Static PIE is valid, but a shared object with no executable entry
            # point (or an entry outside executable file-backed LOAD data) is not.
            ${pkgs.binutils}/bin/readelf -W -l "$source" | ${pkgs.gawk}/bin/awk -v entry="$entry" '
              BEGIN { entry = strtonum(entry) }
              $1 == "LOAD" && entry != 0 {
                flags = ""
                for (i = 7; i < NF; i++) flags = flags $i
                offset = entry - strtonum($3)
                if (index(flags, "E") && offset >= 0 && offset < strtonum($5) && offset < strtonum($6))
                  loadable = 1
              }
              END { exit !loadable }
            ' || {
              echo "upstream binary has no loadable executable entry point: $source" >&2
              exit 1
            }
            install -m0755 -- "$source" "$out/bin"/${escapeShellArg name}
          '')
          validated}
        runHook postInstall
      '';
      passthru = {inherit targetSystem;};
      inherit meta;
    })))
