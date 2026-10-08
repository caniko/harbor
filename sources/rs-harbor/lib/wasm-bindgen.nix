# Resolve the wasm-bindgen CLI that matches the package lockfile.
#
# Do not silently fall back to nixpkgs.wasm-bindgen-cli: wasm-bindgen's CLI and
# generated bindings are an ABI pair, so a loose fallback can produce a bundle
# that only fails in the browser.
{
  lib,
  pkgs,
  cargoLock,
  wasmBindgenCli ? null,
  allowMismatch ? false,
}: let
  lockPath =
    if builtins.isPath cargoLock
    then cargoLock
    else cargoLock.lockFile or cargoLock;
  lines = lib.splitString "\n" (builtins.readFile lockPath);

  # Cargo.lock files in real workspaces can contain tens of thousands of
  # lines. A recursive list walk overflows Nix's evaluator stack; fold over a
  # generated index list instead so the lookup remains strict and bounded.
  wasmBindgenIndex =
    builtins.foldl'
    (found: index:
      if found != null
      then found
      else if builtins.match "name = \"wasm-bindgen\"" (builtins.elemAt lines index) != null
      then index
      else null)
    null
    (builtins.genList (index: index) (builtins.length lines));

  blockStart =
    if wasmBindgenIndex == null
    then null
    else let
      walk = index:
        if index < 0
        then null
        else if builtins.match "[[][[]package[]][]]" (builtins.elemAt lines index) != null
        then index
        else walk (index - 1);
    in
      walk (wasmBindgenIndex - 1);

  blockEnd =
    if blockStart == null
    then null
    else let
      walk = index:
        if index >= builtins.length lines
        then builtins.length lines
        else if
          index
          != blockStart
          && builtins.match "[[][[]package[]][]]" (builtins.elemAt lines index) != null
        then index
        else walk (index + 1);
    in
      walk (blockStart + 1);

  blockLines =
    if blockStart == null
    then []
    else lib.sublist blockStart (blockEnd - blockStart) lines;

  versionLine =
    lib.findFirst
    (line: builtins.match "version = \".*\"" line != null)
    null
    blockLines;

  lockedVersion =
    if versionLine == null
    then null
    else let
      match = builtins.match "version = \"(.*)\"" versionLine;
    in
      if match == null
      then null
      else builtins.head match;

  versionToAttr = version: let
    parts = lib.splitString "." version;
  in "wasm-bindgen-cli_${builtins.elemAt parts 0}_${builtins.elemAt parts 1}_${builtins.elemAt parts 2}";

  attrName =
    if lockedVersion == null
    then null
    else versionToAttr lockedVersion;
  exactPackage =
    if attrName != null && pkgs ? ${attrName}
    then pkgs.${attrName}
    else null;
  suppliedVersion =
    if wasmBindgenCli == null
    then null
    else wasmBindgenCli.version or null;
  versionMismatch =
    suppliedVersion
    != null
    && lockedVersion != null
    && suppliedVersion != lockedVersion;
  available =
    builtins.filter
    (name: builtins.match "wasm-bindgen-cli_.*" name != null)
    (builtins.attrNames pkgs);
in
  assert lib.assertMsg (lockedVersion != null)
  "rs-harbor.resolveWasmBindgenCli: Cargo.lock has no wasm-bindgen package";
  assert lib.assertMsg (!versionMismatch || allowMismatch)
  "rs-harbor.resolveWasmBindgenCli: supplied wasm-bindgen-cli ${toString suppliedVersion} does not match Cargo.lock ${toString lockedVersion}";
  assert lib.assertMsg (wasmBindgenCli != null || exactPackage != null)
  "rs-harbor.resolveWasmBindgenCli: Cargo.lock requires wasm-bindgen ${lockedVersion} (${attrName}), but nixpkgs has none of: ${lib.concatStringsSep ", " available}"; {
    version = lockedVersion;
    inherit attrName;
    package =
      if wasmBindgenCli != null
      then wasmBindgenCli
      else exactPackage;
  }
