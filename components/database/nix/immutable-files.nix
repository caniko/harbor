{lib}: let
  storagePaths = import ./storage-paths.nix {inherit lib;};
in {
  storePath = path:
    storagePaths.normalizedAbsolute path
    && builtins.match "/nix/store/[0-9a-z]{32}-[^/]+(/[^/]+)*" path != null;
  # Keep file inspection in realization, so immutable adapter packages need not
  # exist during module evaluation or require import-from-derivation.
  checkedManifest = label: files: document: let
    executableValidation =
      lib.concatMapStringsSep "\n" (path: ''
        if ! test -f ${lib.escapeShellArg path}; then
          echo ${lib.escapeShellArg "${label} executableFiles must name regular immutable files."} >&2
          exit 1
        fi
        case "$(realpath -e -- ${lib.escapeShellArg path})" in
          /nix/store/*) ;;
          *) echo ${lib.escapeShellArg "${label} executableFiles must resolve inside the immutable store."} >&2; exit 1 ;;
        esac
      '')
      files;
  in
    document.overrideAttrs (previous: {
      buildCommand = executableValidation + previous.buildCommand;
      passthru = (previous.passthru or {}) // {inherit executableValidation;};
    });
}
