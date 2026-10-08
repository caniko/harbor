# harbor-macos-sdk-pin

Single source of truth for the realized macOS SDK store path and the Attic
cache that serves it. A pure-data Nix flake — no inputs, no per-system
outputs, no build steps.

## Why this exists

The macOS SDK derivation in [osxcross](https://github.com/caniko/osxcross)
is a **fixed-output derivation** keyed on a recursive NAR hash. That means
the realized store path is a function of `(name, outputHash)` only — it is
identical on every Nix system that builds (or substitutes) the SDK.

Consumers want one stable pointer to that path so they can:

- avoid rebuilding the SDK on every host,
- avoid carrying the (license-restricted) source archive,
- bump the SDK in exactly one place when a new version is published.

This flake is that pointer.

## Outputs

```nix
{
  sdkVersion = "26.1";
  outputHash = "sha256-...=";          # recursive NAR hash of the SDK
  storePath  = "/nix/store/...-macosx-sdk-26.1";
  attic = {
    cacheName = "harbor-macos-sdk";
    url       = "https://attic.candee.baby/harbor-macos-sdk";
    publicKey = "harbor-macos-sdk:...=";
  };
}
```

All values are constants. No `flake-utils.lib.eachDefaultSystem`, no
`builtins.currentSystem` — the same pin serves `x86_64-linux`,
`aarch64-linux`, `*-darwin`, etc.

## Consuming

```nix
{
  inputs.harbor-macos-sdk-pin.url =
    "git+ssh://git@github.com/caniko/harbor-macos-sdk-pin.git";

  outputs = { harbor-macos-sdk-pin, ... }: let
    pin = harbor-macos-sdk-pin;
  in {
    # Thread the storePath into a project flake that supports it (e.g.
    # harbor-rs's mkCross via `macosSdkStorePath`).
    # ...
  };
}
```

To substitute the path without rebuilding, the consumer's Nix config must
trust the Attic cache:

```
extra-substituters       = https://attic.candee.baby/harbor-macos-sdk
extra-trusted-public-keys = harbor-macos-sdk:...=
```

Project flakes can advertise this through `nixConfig.extra-substituters`
and `extra-trusted-public-keys` — users still need
`accept-flake-config = true` (or one-time prompt approval) for the
substituter to be used.

## Privacy

The macOS SDK is Apple-licensed. The `harbor-macos-sdk` Attic cache MUST
stay private — authenticated pulls only, no anonymous access, never
mirrored to a public substituter.

## Bumping the pin

When a new SDK version is published to the cache:

1. Run `nix run harbor-rs#publish-macos-sdk -- --archive ... --version <new>`
   on a builder with access to the source archive.
2. Copy the printed `STORE_PATH`, `RECURSIVE_HASH`, and `SDK_VERSION` into
   [flake.nix](./flake.nix).
3. Commit and push. Downstream flakes pick it up with
   `nix flake update harbor-macos-sdk-pin`.
