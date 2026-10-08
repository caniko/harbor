# Shared validation and reproducibility primitives for release helpers.
{lib}: let
  inherit (lib) escapeShellArg;
in rec {
  require = label: value:
    assert lib.assertMsg (value != null) "rs-harbor: release ${label} is required"; value;

  requireString = label: value:
    assert lib.assertMsg (builtins.isString value && value != "")
    "rs-harbor: release ${label} must be a non-empty string"; value;

  requireNonEmpty = context: value:
    assert lib.assertMsg (lib.isString value && value != "")
    "rs-harbor: ${context} must be a non-empty string"; value;

  requireBinaries = {
    context ? "binary release",
    binaries,
  }:
    assert lib.assertMsg (lib.isList binaries && binaries != [])
    "rs-harbor: ${context} binaries must be a non-empty list";
      map (binary: requireNonEmpty "${context} binary" binary) binaries;

  expectedMachine = {
    system,
    context ? "release ELF",
  }:
    if system == "x86_64-linux"
    then "Advanced Micro Devices X86-64"
    else if system == "aarch64-linux"
    then "AArch64"
    else throw "rs-harbor: unsupported ${context} system '${system}'";

  deterministicTarFlags = "--sort=name --owner=0 --group=0 --numeric-owner";

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
