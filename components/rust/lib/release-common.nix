# Shared validation and reproducibility primitives for release helpers.
{lib}: let
  inherit (lib) escapeShellArg;
in rec {
  require = label: value:
    assert lib.assertMsg (value != null) "harbor-rs: release ${label} is required"; value;

  requireString = label: value:
    assert lib.assertMsg (builtins.isString value && value != "")
    "harbor-rs: release ${label} must be a non-empty string"; value;

  requireNonEmpty = context: value:
    assert lib.assertMsg (lib.isString value && value != "")
    "harbor-rs: ${context} must be a non-empty string"; value;

  requireBinaries = {
    context ? "binary release",
    binaries,
  }:
    assert lib.assertMsg (lib.isList binaries && binaries != [])
    "harbor-rs: ${context} binaries must be a non-empty list";
      map (binary: requireNonEmpty "${context} binary" binary) binaries;

  expectedMachine = {
    system,
    context ? "release ELF",
  }:
    if system == "x86_64-linux"
    then "Advanced Micro Devices X86-64"
    else if system == "aarch64-linux"
    then "AArch64"
    else throw "harbor-rs: unsupported ${context} system '${system}'";

  deterministicTarFlags = "--sort=name --owner=0 --group=0 --numeric-owner";

  validRelativePath = path:
    builtins.isString path
    && path != ""
    && !(lib.hasPrefix "/" path)
    && !(lib.hasInfix "\n" path)
    && !(lib.hasInfix "\r" path)
    && lib.all (part: part != "" && part != "." && part != "..") (lib.splitString "/" path);

  pathsOverlap = a: b: a == b || lib.hasPrefix "${a}/" b || lib.hasPrefix "${b}/" a;

  # Named regular files; directories are created by install, never copied wholesale.
  stageFiles = {
    files,
    root,
    reserved ? [],
  }: let
    names = builtins.attrNames files;
    validate = name: let
      spec = files.${name};
      mode = spec.mode or "0644";
    in
      assert lib.assertMsg (validRelativePath name) "harbor-rs: invalid release file destination '${name}'";
      assert lib.assertMsg (lib.all (other: !(pathsOverlap name other)) reserved) "harbor-rs: release file '${name}' collides with a reserved path";
      assert lib.assertMsg (lib.all (other: name == other || !(pathsOverlap name other)) names) "harbor-rs: release file '${name}' overlaps another destination";
      assert lib.assertMsg (builtins.isAttrs spec && spec ? source) "harbor-rs: release file '${name}' requires source";
      assert lib.assertMsg (builtins.isString mode && builtins.match "0[0-7][0-7][0-7]" mode != null) "harbor-rs: release file '${name}' has an invalid mode"; "install -D -m ${escapeShellArg mode} ${escapeShellArg (toString spec.source)} \"${root}\"/${escapeShellArg name}";
  in
    lib.concatMapStringsSep "\n" validate names;

  staticElfValidation = {
    readelf,
    grep,
    path,
    machine,
    label ? "release binary",
  }: ''
    ${readelf} -h ${path} | ${grep} -F ${escapeShellArg machine} >/dev/null || {
      echo "${label} has the wrong ELF machine: ${path}" >&2
      exit 1
    }
    if ${readelf} -l ${path} | ${grep} -q 'INTERP'; then
      echo "${label} is dynamically linked: ${path}" >&2
      exit 1
    fi
    if ${readelf} -d ${path} | ${grep} -q 'NEEDED'; then
      echo "${label} has dynamic dependencies: ${path}" >&2
      exit 1
    fi
  '';
}
