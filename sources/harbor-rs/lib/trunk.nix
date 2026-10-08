# mkTrunkPackage :: {
#   pkgs, src, cargoLock, pname,
#   craneLib ?, cargoExtraArgs ? "",
#   nativeBuildInputs ? [], ...
# } -> derivation
#
# Build a WASM front-end distribution with trunk.
#
# Two modes:
#   1. Pragmatic (omitted craneLib): auto-creates a wasm-capable
#      craneLib via mkWasmToolchain.
#   2. Modular (supplied craneLib): uses the caller's craneLib
#      directly — caller must ensure wasm32-unknown-unknown is
#      in the toolchain targets.
#
# Cargo.lock is parsed to auto-select the matching
# wasm-bindgen-cli from nixpkgs.
{
  crane,
  packageTests,
}: {
  src,
  pkgs,
  cargoLock,
  pname,
  version ? "0.1.0",
  craneLib ? null,
  cargoExtraArgs ? "",
  nativeBuildInputs ? [],
  ...
} @ args: let
  lib = pkgs.lib;

  lockPath =
    if builtins.isPath cargoLock
    then cargoLock
    else cargoLock.lockFile or cargoLock;
  resolvedWasmBindgen = import ./wasm-bindgen.nix {
    inherit lib pkgs cargoLock;
  };
  wasmBindgenVersion = resolvedWasmBindgen.version;
  resolvedCli = resolvedWasmBindgen.package;

  effectiveCraneLib =
    if craneLib != null
    then craneLib
    else (import ./wasm-toolchain.nix {inherit crane pkgs;}).craneLib;

  cleanWasmSrc = lib.cleanSourceWith {
    inherit src;
    filter = path: type:
      (effectiveCraneLib.filterCargoSources path type)
      || type
      == "regular"
      && pkgs.lib.any
      (ext: pkgs.lib.hasSuffix ext (baseNameOf (toString path)))
      [".html" ".css" ".js"]
      || baseNameOf (toString path) == "Trunk.toml";
    name = "${pname}-wasm-src";
  };

  extraArgs = builtins.removeAttrs args [
    "src"
    "pkgs"
    "cargoLock"
    "pname"
    "version"
    "craneLib"
    "cargoExtraArgs"
    "nativeBuildInputs"
  ];
in let
  package = effectiveCraneLib.buildTrunkPackage ({
      src = cleanWasmSrc;
      inherit pname version;
      cargoLock = lockPath;
      cargoExtraArgs = cargoExtraArgs;
      wasm-bindgen-cli = resolvedCli;
      nativeBuildInputs =
        nativeBuildInputs
        ++ [
          pkgs.clang
          pkgs.mold
          pkgs.nodejs
          pkgs.tailwindcss_4
        ];
    }
    // extraArgs);
in
  package
  // {
    artifactBuilder = packageTests.mkArtifactBuilder {
      kind = "trunk-builder";
      packageName = pname;
      inherit version;
      output = toString package;
      buildCommand = "nix build .#${pname}-trunk";
      inputs = [(toString src)];
      metadata = {
        inherit cargoExtraArgs wasmBindgenVersion;
        helper = "mkTrunkPackage";
      };
    };
  }
