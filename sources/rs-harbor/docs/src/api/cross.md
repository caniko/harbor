# mkCross

`mkCross` assembles the cross-compilation helpers for Windows and macOS.

## Windows support

MinGW toolchain components are exposed through the returned environment and can be enabled in dev shells without extra project-specific setup.

## aarch64 Linux support

The returned `linuxAarch64` record lifts the `aarch64-unknown-linux-gnu` cross
boilerplate out of consumer flakes:

- `linuxAarch64.cc`: the `pkgsCross.aarch64-multiplatform` stdenv `cc`
- `linuxAarch64.pkgsCross`: the full `pkgsCross.aarch64-multiplatform` package set
  (handy for `buildInputs` such as cross `openssl`/`wayland`)
- `linuxAarch64.env`: a ready-to-merge attrset with
  `CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER`,
  `CC_aarch64_unknown_linux_gnu`, and `CXX_aarch64_unknown_linux_gnu`

Merge `linuxAarch64.env` into a crane args set (along with
`CARGO_BUILD_TARGET = "aarch64-unknown-linux-gnu"` and
`PKG_CONFIG_ALLOW_CROSS = "1"`) to cross-build for ARM64 Linux, or let
[`mkCrossPackages`](./cross-packages.md) wire it for you.

## macOS support

osxcross is optional and turns on when one of these inputs is available:

- `macosSdk`: an already realized SDK reference
- `macosSdkStorePath`: explicit SDK store path, normally injected by host configuration
- `macosSdkOutputHash`: the SDK's recursive output hash; **pass it together with `macosSdkStorePath`** so the SDK becomes a sandbox-mounted build input (see the warning below)
- `sdkArchive`: a local SDK archive path
- `macosSdkEnvPath`: explicit local discovery path
- `MACOS_SDK`: opt-in local discovery fallback when `enableImpureMacosSdkEnv = true`

Only one of `macosSdkStorePath`, `sdkArchive`, or `macosSdk` can be provided at a time.
Resolution precedence is `macosSdk`, then `macosSdkStorePath`, then `sdkArchive`, then `macosSdkEnvPath`.

> **Pass `macosSdkOutputHash` with `macosSdkStorePath`.** A bare store path carries no Nix string context, so under `sandbox = true` it is never bind-mounted and osxcross-clang fails with `cannot find macOS SDK` — with no eval-time error. The hash lets rs-harbor reconstruct the SDK as a content-addressed fixed-output derivation that resolves to the same path _with_ context, making it a real sandbox input. See [macOS SDK Initialization](../reference/macos-sdk.md#always-pass-macossdkoutputhash-too-the-sandbox-gotcha).

Ad-hoc signing of darwin outputs works out of the box: the osxcross toolchain provides an unprefixed `codesign_allocate` on `PATH`, so `sigtool`-style `codesign` in a `postInstall` needs no `CODESIGN_ALLOCATE` wiring. See [Ad-hoc signing darwin binaries](../reference/macos-sdk.md#ad-hoc-signing-darwin-binaries).

`macosSdkEnvPath` and the opt-in `MACOS_SDK` fallback accept a direct `MacOSX<version>.sdk` directory, a parent directory containing that SDK, or a supported SDK archive. Directory inputs are validated for the SDK settings file, `TargetConditionals.h`, and the required CoreFoundation/SystemConfiguration frameworks before osxcross is invoked.

## Example

```nix
cross = rs-harbor.lib.mkCross {
  inherit pkgs system;
};
```

See [macOS SDK Initialization](../reference/macos-sdk.md) for the recommended way to create and inject `macosSdkStorePath` from host configuration.
