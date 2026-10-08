# mkDebPackage :: { pkgs, packageName, version, arch, maintainer, description, files, ... } -> derivation
#
# Build a simple binary .deb from explicit staged files. This helper is for
# projects that already have built artifacts and need deterministic release
# packages without invoking cargo-deb.
{packageTests}: {
  pkgs,
  packageName,
  version,
  arch,
  maintainer,
  description,
  depends ? [],
  section ? "utils",
  priority ? "optional",
  files,
}: let
  inherit (pkgs) lib;

  validName = builtins.isString packageName && builtins.match "[a-z0-9][a-z0-9+.-]+" packageName != null;
  validVersion = builtins.isString version && version != "";
  validArch = builtins.elem arch ["amd64" "arm64" "all"];
  validMaintainer = builtins.isString maintainer && maintainer != "";
  validDescription = builtins.isString description && description != "";
  validFiles = builtins.isList files && files != [];

  validateFile = file:
    assert builtins.isAttrs file || throw "mkDebPackage: each file entry must be an attrset";
    assert file ? source || throw "mkDebPackage: file entry missing source";
    assert file ? target || throw "mkDebPackage: file entry missing target";
    assert builtins.isString file.target
    && lib.hasPrefix "/" file.target
    || throw "mkDebPackage: file target must be an absolute package path";
    assert !(lib.hasPrefix "/DEBIAN/" file.target)
    || throw "mkDebPackage: file target must not write into /DEBIAN"; true;

  fileChecks = map validateFile files;
  controlText =
    ''
      Package: ${packageName}
      Version: ${version}
      Architecture: ${arch}
      Maintainer: ${maintainer}
      Section: ${section}
      Priority: ${priority}
    ''
    + lib.optionalString (depends != []) ''
      Depends: ${lib.concatStringsSep ", " depends}
    ''
    + ''
      Description: ${description}
    '';

  installCommands =
    lib.concatMapStringsSep "\n" (file: let
      mode = file.mode or "0644";
      target = lib.removePrefix "/" file.target;
    in ''
      install -D -m ${lib.escapeShellArg mode} ${lib.escapeShellArg (toString file.source)} "$pkg"/${lib.escapeShellArg target}
    '')
    files;
  package =
    pkgs.runCommand "${packageName}_${version}_${arch}.deb" {
      nativeBuildInputs = [pkgs.dpkg];
    } ''
      set -euo pipefail
      pkg="$TMPDIR/pkg"
      mkdir -p "$pkg/DEBIAN"
      printf '%s\n' ${lib.escapeShellArg controlText} > "$pkg/DEBIAN/control"
      ${installCommands}
      dpkg-deb --root-owner-group --build "$pkg" "$out"
    '';
in
  assert validName || throw "mkDebPackage: packageName must be a Debian package name, got ${packageName}";
  assert validVersion || throw "mkDebPackage: version must be non-empty";
  assert validArch || throw "mkDebPackage: arch must be one of amd64, arm64, or all";
  assert validMaintainer || throw "mkDebPackage: maintainer must be non-empty";
  assert validDescription || throw "mkDebPackage: description must be non-empty";
  assert validFiles || throw "mkDebPackage: files must be a non-empty list";
  assert lib.all (x: x) fileChecks;
    package
    // {
      artifactBuilder = packageTests.mkArtifactBuilder {
        kind = "debian-builder";
        inherit packageName version;
        output = toString package;
        buildCommand = "nix build .#${packageName}-deb";
        metadata = {
          inherit arch depends section priority;
          fileCount = builtins.length files;
          helper = "mkDebPackage";
        };
      };
    }
