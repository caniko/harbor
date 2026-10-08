# mkCross :: { pkgs, system, macosSdkStorePath?, sdkArchive?, macosSdk?, macosSdkOutputHash?, macosSdkEnvPath?, enableImpureMacosSdkEnv?, enableOsxcross?, osxSdkVersion? }
#         -> { mingwCC, mingwBinutils, winpthreads, windowsEnv,
#              linuxAarch64, macosSdk, osxcrossToolchain, osxcrossRustHelpers }
#
# Build cross-compilation toolchains (MinGW for Windows, aarch64 Linux GNU,
# osxcross for macOS).
{osxcross}: {
  pkgs,
  system,
  macosSdkStorePath ? null,
  sdkArchive ? null,
  macosSdk ? null,
  macosSdkOutputHash ? null,
  enableImpureMacosSdkEnv ? false,
  macosSdkEnvPath ?
    if enableImpureMacosSdkEnv
    then builtins.getEnv "MACOS_SDK"
    else "",
  enableOsxcross ? (macosSdkStorePath != null || sdkArchive != null || macosSdk != null || macosSdkEnvPath != ""),
  osxSdkVersion ? "26.1",
}:
if
  (
    if macosSdk != null
    then 1
    else 0
  )
  + (
    if macosSdkStorePath != null
    then 1
    else 0
  )
  + (
    if sdkArchive != null
    then 1
    else 0
  )
  > 1
then throw "rs-harbor.mkCross: pass only one of macosSdk, macosSdkStorePath, or sdkArchive"
else let
  lib = pkgs.lib;
  mingwCC = pkgs.pkgsCross.mingwW64.stdenv.cc;
  mingwBinutils = pkgs.pkgsCross.mingwW64.stdenv.cc.bintools.bintools;
  winpthreads = pkgs.pkgsCross.mingwW64.windows.pthreads;

  # aarch64-unknown-linux-gnu cross toolchain. Exposes the cross stdenv cc and
  # a ready-to-merge env attrset so consumers can build for ARM64 Linux without
  # hand-rolling the CC_/CXX_/linker env vars (see rs-modde flake.nix).
  linuxAarch64Cross = pkgs.pkgsCross.aarch64-multiplatform;
  linuxAarch64CC = linuxAarch64Cross.stdenv.cc;
  linuxAarch64 = {
    cc = linuxAarch64CC;
    pkgsCross = linuxAarch64Cross;
    env = {
      CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER = "${linuxAarch64CC}/bin/${linuxAarch64CC.targetPrefix}cc";
      CC_aarch64_unknown_linux_gnu = "${linuxAarch64CC}/bin/${linuxAarch64CC.targetPrefix}cc";
      CXX_aarch64_unknown_linux_gnu = "${linuxAarch64CC}/bin/${linuxAarch64CC.targetPrefix}c++";
    };
  };

  supportsOsxcross = system == "x86_64-linux";

  archiveSuffixes = [
    ".tar"
    ".tar.gz"
    ".tgz"
    ".tar.xz"
    ".txz"
    ".tar.bz2"
    ".tbz2"
  ];

  hasArchiveSuffix = value:
    lib.any (suffix: lib.hasSuffix suffix (toString value)) archiveSuffixes;

  requireAbsolutePath = context: value: let
    str = toString value;
  in
    if lib.hasPrefix "/" str
    then str
    else throw "rs-harbor.mkCross: ${context} must be an absolute path, got '${str}'";

  pathExistsAbs = value: let
    str = toString value;
  in
    lib.hasPrefix "/" str && builtins.pathExists (/. + str);

  sdkRootFor = value:
    if lib.hasSuffix ".sdk" (toString value)
    then toString value
    else "${toString value}/MacOSX${osxSdkVersion}.sdk";

  requiredSdkEntries = [
    "SDKSettings.json"
    "usr/include/TargetConditionals.h"
    "System/Library/Frameworks"
    "System/Library/Frameworks/SystemConfiguration.framework"
    "System/Library/Frameworks/CoreFoundation.framework"
  ];

  missingSdkEntries = sdkRoot:
    lib.filter (entry: !(pathExistsAbs "${toString sdkRoot}/${entry}")) requiredSdkEntries;

  validateVisibleSdkRoot = context: sdkRoot: let
    root = toString sdkRoot;
    missing = missingSdkEntries root;
  in
    if !(pathExistsAbs root)
    then root
    else if missing == []
    then root
    else
      throw ''
        rs-harbor.mkCross: ${context} is not a complete macOS SDK.
        SDK root: ${root}
        Missing required entries: ${lib.concatStringsSep ", " missing}
        Expected a real Apple SDK root such as MacOSX${osxSdkVersion}.sdk, not a fake/minimal fixture.
      '';

  # A pinned macOS SDK store path (e.g. from rs-harbor-macos-sdk-pin) is just a
  # string, so it carries no Nix string context and is therefore NOT a build
  # input: under `sandbox = true` the daemon never bind-mounts it and
  # osxcross-clang fails inside the build with "cannot find macOS SDK", even
  # though the path is valid in the store. The SDK is a fixed-output derivation
  # whose output path is fully determined by (name, outputHash, outputHashMode),
  # so reconstructing that FOD here yields the identical store path *with*
  # context. It then becomes a real dependency of consuming derivations and is
  # mounted into the sandbox. When the path is already realized (pinned, or
  # substituted from the harbor-macos-sdk cache) Nix uses it directly and never
  # runs the builder; when it is missing and unsubstitutable the build fails
  # early with the clear message below instead of a cryptic osxcross error.
  #
  # Requires the caller to pass `macosSdkOutputHash` (rs-harbor-macos-sdk-pin
  # exposes it as `.outputHash`). Without it we fall back to the legacy
  # context-free string, which is correct only outside the sandbox / on a build
  # host where the SDK already lives on disk.
  mkPinnedSdkFod = storePath: outputHash: let
    fod = derivation {
      inherit system;
      name = "macosx-sdk-${osxSdkVersion}";
      builder = "/bin/sh";
      args = [
        "-c"
        "echo 'rs-harbor.mkCross: macOS SDK ${toString storePath} is not in the store and no sdkArchive was provided. Add the harbor-macos-sdk Attic cache as a (daemon) substituter so it can be fetched, or pass sdkArchive to mkCross.' >&2; exit 1"
      ];
      outputHashMode = "recursive";
      outputHashAlgo = "sha256";
      inherit outputHash;
    };
  in
    if fod.outPath != toString storePath
    then
      throw ''
        rs-harbor.mkCross: reconstructed macOS SDK FOD resolves to
          ${fod.outPath}
        but macosSdkStorePath is
          ${toString storePath}
        Ensure macosSdkOutputHash and osxSdkVersion match the
        rs-harbor-macos-sdk-pin revision you locked.''
    else fod;

  mkStorePathSdkRef = storePath: let
    path = toString storePath;
    # Validate the realized SDK layout when it is present on disk (no-op when
    # the path is not yet realized).
    root = validateVisibleSdkRoot "macosSdkStorePath" (sdkRootFor path);
    sdkInput =
      if macosSdkOutputHash != null
      then mkPinnedSdkFod storePath macosSdkOutputHash
      else storePath;
  in
    osxcross.lib.${system}.mkMacosSdkRef {
      sdk = sdkInput;
      # Same path as `root`, but interpolated from `sdkInput` so it carries
      # context (and thus becomes a sandbox-mounted input) when the FOD
      # reconstruction is in play.
      sdkRoot =
        if macosSdkOutputHash != null
        then "${sdkInput}/MacOSX${osxSdkVersion}.sdk"
        else root;
      sdkVersion = osxSdkVersion;
    };

  mkEnvSdkRef = envPath: let
    path = requireAbsolutePath "MACOS_SDK" envPath;
    directRoot = path;
    parentRoot = "${path}/MacOSX${osxSdkVersion}.sdk";
  in
    if pathExistsAbs directRoot && lib.hasSuffix ".sdk" directRoot
    then let
      checkedSdkRoot = validateVisibleSdkRoot "MACOS_SDK direct SDK directory" directRoot;
      sdkPath = /. + path;
    in
      assert checkedSdkRoot != "";
        osxcross.lib.${system}.mkMacosSdkRef {
          sdk = sdkPath;
          sdkRoot = sdkPath;
          sdkVersion = osxSdkVersion;
        }
    else if pathExistsAbs parentRoot
    then let
      checkedSdkRoot = validateVisibleSdkRoot "MACOS_SDK parent directory" parentRoot;
      sdkParent = /. + path;
      sdkRoot = "${sdkParent}/MacOSX${osxSdkVersion}.sdk";
    in
      assert checkedSdkRoot != "";
        osxcross.lib.${system}.mkMacosSdkRef {
          sdk = sdkParent;
          inherit sdkRoot;
          sdkVersion = osxSdkVersion;
        }
    else if pathExistsAbs path && hasArchiveSuffix path
    then
      osxcross.lib.${system}.mkMacosSdk {
        sdkArchive = /. + path;
        sdkVersion = osxSdkVersion;
        outputHash = macosSdkOutputHash;
      }
    else
      throw ''
        rs-harbor.mkCross: MACOS_SDK must point to one of:
        - a MacOSX${osxSdkVersion}.sdk directory
        - a parent directory containing MacOSX${osxSdkVersion}.sdk
        - a supported SDK archive (${lib.concatStringsSep ", " archiveSuffixes})
        Got: ${path}
      '';

  effectiveMacosSdk =
    if !enableOsxcross || !supportsOsxcross
    then null
    else if macosSdk != null
    then macosSdk
    else if macosSdkStorePath != null
    then mkStorePathSdkRef macosSdkStorePath
    else if sdkArchive != null
    then
      osxcross.lib.${system}.mkMacosSdk {
        inherit sdkArchive;
        sdkVersion = osxSdkVersion;
        outputHash = macosSdkOutputHash;
      }
    else if macosSdkEnvPath != ""
    then mkEnvSdkRef macosSdkEnvPath
    else throw "rs-harbor.mkCross: enableOsxcross is true, but no macOS SDK input was provided";

  osxcrossArgs =
    {
      sdkVersion = osxSdkVersion;
    }
    // pkgs.lib.optionalAttrs (effectiveMacosSdk != null) {
      macosSdk = effectiveMacosSdk;
    };

  osxcrossToolchain =
    if enableOsxcross && supportsOsxcross
    then osxcross.lib.${system}.mkOsxcross osxcrossArgs
    else null;

  osxcrossRustHelpers =
    if osxcrossToolchain != null
    then osxcross.lib.${system}.mkRustHelpers osxcrossToolchain
    else null;

  # Pre-built attrset of Windows cross-compilation env vars.
  # Consumers can merge this into mkShell when needed, without
  # polluting every dev shell unconditionally.
  windowsEnv = {
    CC_x86_64_pc_windows_gnu = "${mingwCC}/bin/x86_64-w64-mingw32-gcc";
    CXX_x86_64_pc_windows_gnu = "${mingwCC}/bin/x86_64-w64-mingw32-g++";
    AR_x86_64_pc_windows_gnu = "${mingwCC}/bin/x86_64-w64-mingw32-ar";
    CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "${mingwCC}/bin/x86_64-w64-mingw32-gcc";
    CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS = "-L ${winpthreads}/lib";
  };
in {
  inherit mingwCC mingwBinutils winpthreads;
  inherit linuxAarch64;
  macosSdk = effectiveMacosSdk;
  inherit osxcrossToolchain osxcrossRustHelpers;
  inherit windowsEnv;
}
