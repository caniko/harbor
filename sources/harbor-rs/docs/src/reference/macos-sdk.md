# macOS SDK Initialization

For version-controlled project flakes, `harbor-rs` supports an explicit `macosSdkStorePath`, but that path is host-specific. Reusable flakes should accept it from host configuration instead of committing it directly. harbor-rs owns SDK discovery and validation; osxcross receives only the resolved SDK reference.

## Initialize once per host

```bash
nix run harbor-rs#init-macos-sdk -- /host/local/MacOSX26.1.sdk.tar.xz 26.1
```

The command realizes the archive, validates the SDK root, and prints:

- the host-specific store path for host configuration
- the resolved SDK root
- the recursive hash used for the fixed-output rebuild

Validation requires:

- `SDKSettings.json`
- `usr/include/TargetConditionals.h`
- `System/Library/Frameworks`
- `SystemConfiguration.framework`
- `CoreFoundation.framework`

## Inject the printed store path

```nix
canix.development.macosSdk.storePath = "/nix/store/<host-sdk>-macosx-sdk-26.1";
canix.development.macosSdk.sdkVersion = "26.1";
```

Host wrappers can then pass the value to `mkCross` as `macosSdkStorePath`. Standalone project flakes should keep it `null` so native and non-macOS cross outputs evaluate everywhere. On another machine, either run the same initialization flow with that host's local archive or fetch the realized store path from your binary cache.

## Always pass `macosSdkOutputHash` too (the sandbox gotcha)

A bare `macosSdkStorePath` is a string with **no Nix string context**, so on its own it is _not_ a build input. Under `sandbox = true` the daemon never bind-mounts it and osxcross-clang fails inside the sandboxed build with `cannot find macOS SDK`, even though the path is valid in the store. Passing only `macosSdkStorePath` silently produces this broken state — there is no error at eval time.

Always pass `macosSdkOutputHash` alongside it (the recursive hash printed by `init-macos-sdk`):

```nix
cross = harbor-rs.lib.mkCross {
  inherit pkgs system;
  macosSdkStorePath = "/nix/store/<host-sdk>-macosx-sdk-26.1";
  macosSdkOutputHash = "sha256-…";   # recursive hash from init-macos-sdk
  osxSdkVersion = "26.1";
};
```

With the hash, harbor-rs reconstructs the SDK's fixed-output derivation. Its output path is fully determined by `(name, outputHash, recursive sha256)`, so it resolves to the _identical_ store path — but now _with_ context, making the SDK a real, sandbox-mounted dependency. harbor-rs asserts the reconstructed path equals `macosSdkStorePath`, so a hash/version mismatch fails loudly at eval instead of silently. Already-realized paths are used directly (no rebuild); a missing, unsubstitutable SDK fails early with a clear message.

Because the reconstruction is a recursive-SHA256 fixed-output derivation, its output is **content-addressed and signature-exempt**: any substituter serving it (e.g. a private Attic cache) can provide it without a trusted signature. The substituter URL must still be in the daemon's effective `substituters` list, though — a flake `nixConfig.extra-substituters` only takes effect with `--accept-flake-config` or for a trusted user.

## Ad-hoc signing darwin binaries

The osxcross toolchain installs an unprefixed `codesign_allocate` on `PATH` (next to the arch-prefixed `<arch>-apple-<target>-codesign_allocate`), mirroring macOS's `/usr/bin/codesign_allocate`. Tools that ad-hoc sign Mach-O outputs — e.g. `sigtool`'s `codesign --sign - --force` in a `postInstall` — spawn `codesign_allocate` by bare name. With the osxcross toolchain in `nativeBuildInputs` (as `mkCrossPackages` and the darwin cross-builders arrange), that bare-name spawn resolves on `PATH`, so **no `CODESIGN_ALLOCATE` wiring is needed**. Sign in `postInstall`, before `fixupPhase`'s strip leaves the signature in place.

## Local discovery with `MACOS_SDK`

For local impure workflows, `mkCross` can read `MACOS_SDK` when `enableImpureMacosSdkEnv = true`. The value may be:

- a direct SDK root, such as `/path/to/MacOSX26.1.sdk`
- a parent directory containing `MacOSX26.1.sdk`
- a supported SDK archive, such as `/path/to/MacOSX26.1.sdk.tar.xz`

Explicit arguments take precedence over `MACOS_SDK`: `macosSdk`, `macosSdkStorePath`, `sdkArchive`, then the environment value.
