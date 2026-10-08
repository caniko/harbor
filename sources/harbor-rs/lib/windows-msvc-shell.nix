# mkWindowsMsvcDevShell :: { pkgs, lib, llvmPackages, toolchain, ... } -> derivation
#
# Dev shell for cross-compiling Rust to x86_64-pc-windows-msvc using
# nixpkgs' xwin-fetched Windows SDK (`pkgs.windows.sdk`), clang-cl, and
# lld-link. Wires up INCLUDE / LIB / SDKROOT and the cargo target env
# vars so `cargo build --release --target x86_64-pc-windows-msvc` works
# without any further configuration.
#
# Consumers may pass:
#   - extraPackages: extra dev-shell tools (default [])
#   - extraEnv:      extra env attributes merged into the shell (default {})
#   - extraShellHook: extra shell-hook fragment appended after SDKROOT (default "")
#   - checks:        attrset of derivations to materialise on shell entry
#                    (passed straight to craneLib.devShell)
{
  pkgs,
  lib,
  llvmPackages,
  toolchain,
  extraPackages ? [],
  extraEnv ? {},
  extraShellHook ? "",
  checks ? {},
}: let
  winSdk = pkgs.windows.sdk;
  clangPath = "${llvmPackages.clang-unwrapped}/bin/clang-cl";
  lldPath = "${llvmPackages.bintools-unwrapped}/bin/lld-link";
  llvmLibPath = "${llvmPackages.bintools-unwrapped}/bin/llvm-lib";
  sdkRoot = "${winSdk}";
  sdkDir = "${winSdk}/sdk";
  crtDir = "${winSdk}/crt";
  includePath = lib.concatStringsSep ";" [
    "${winSdk}/crt/include"
    "${winSdk}/sdk/include/ucrt"
    "${winSdk}/sdk/include/um"
    "${winSdk}/sdk/include/shared"
    "${winSdk}/sdk/include/winrt"
    "${winSdk}/sdk/include/cppwinrt"
  ];
  libPath = lib.concatStringsSep ";" [
    "${winSdk}/crt/lib/x64"
    "${winSdk}/sdk/lib/ucrt/x64"
    "${winSdk}/sdk/lib/um/x64"
  ];
  baseEnv = {
    LIBCLANG_PATH = lib.makeLibraryPath [pkgs.clang.cc];
    INCLUDE = includePath;
    LIB = libPath;
    CC_x86_64_pc_windows_msvc = clangPath;
    CXX_x86_64_pc_windows_msvc = clangPath;
    AR_x86_64_pc_windows_msvc = llvmLibPath;
    CFLAGS_x86_64_pc_windows_msvc = lib.concatStringsSep " " [
      "/winsdkdir"
      sdkDir
      "/vctoolsdir"
      crtDir
    ];
    CXXFLAGS_x86_64_pc_windows_msvc = lib.concatStringsSep " " [
      "/winsdkdir"
      sdkDir
      "/vctoolsdir"
      crtDir
    ];
    CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER = lldPath;
    CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = lib.concatStringsSep " " [
      "-Lnative=${winSdk}/crt/lib/x64"
      "-Lnative=${winSdk}/sdk/lib/ucrt/x64"
      "-Lnative=${winSdk}/sdk/lib/um/x64"
    ];
    BINDGEN_EXTRA_CLANG_ARGS_x86_64_pc_windows_msvc = lib.concatStringsSep " " [
      "--target=x86_64-pc-windows-msvc"
      "/winsdkdir"
      sdkDir
      "/vctoolsdir"
      crtDir
    ];
  };
in
  toolchain.craneLib.devShell ({
      inherit checks;

      packages =
        (with pkgs; [
          cmake
          gcc
          clang
          mold
          lld
          pkg-config
          winSdk
          llvmPackages.clang-unwrapped
          llvmPackages.bintools-unwrapped
        ])
        ++ extraPackages;

      shellHook = ''
                export SDKROOT="${sdkRoot}"
        ${extraShellHook}
      '';
    }
    // baseEnv
    // extraEnv)
