# mkRustNativeBuildInputs :: { pkgs, extra ? [], includeClang ? true, includeMold ? true } -> [package]
#
# Native tooling expected by Rust package derivations. Dev shells already expose
# these tools through mkDevShell; package derivations should carry them too so
# build-script consumers and external drv reproducers can find `cc`/`gcc`.
{
  pkgs,
  extra ? [],
  includeClang ? true,
  includeMold ? true,
}: let
  lib = pkgs.lib;
in
  [
    pkgs.stdenv.cc
  ]
  ++ lib.optionals includeClang [
    pkgs.clang
  ]
  ++ lib.optionals includeMold [
    pkgs.mold
  ]
  ++ extra
