# Install publisher-owned static Linux archives without requiring a Harbor
# manifest or rewriting the executable. Fetching and provenance are caller-owned.
{pkgs}: {
  pname,
  version,
  sources,
  binaries,
  targetSystem ? pkgs.stdenv.hostPlatform.system,
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
    builtins.deepSeq validated (builtins.seq machine (builtins.seq src (pkgs.stdenvNoCC.mkDerivation {
      inherit pname version src;
      # Paths in binaries are relative to the archive root, including any
      # publisher-owned enclosing directory. Never guess or flatten that layout.
      sourceRoot = ".";
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
            test -f "$source" && test -x "$source" || {
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
            install -m0755 -- "$source" "$out/bin"/${escapeShellArg name}
          '')
          validated}
        runHook postInstall
      '';
      passthru = {inherit targetSystem;};
      inherit meta;
    })))
