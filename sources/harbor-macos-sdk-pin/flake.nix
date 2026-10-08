{
  description = "Deployment pin for the realized macOS SDK store path and its Attic cache";

  # Pure data flake — no inputs, no per-system outputs. Every value here is
  # a constant that consumers read directly. Bumping the SDK means editing
  # this file (and only this file), then locking the new revision wherever
  # it's consumed.
  #
  # The store path below is the realized output of osxcross's `mkMacosSdk`
  # called with the matching `outputHash`. Because that derivation is a
  # fixed-output derivation (outputHashMode = "recursive"), the store path
  # is a function of (name, outputHash) only — it is identical on every
  # Nix system and every consumer that substitutes from the cache.
  outputs = _: {
    sdkVersion = "26.1";

    # Recursive NAR sha256 of the SDK directory. Combined with the
    # derivation name `macosx-sdk-${sdkVersion}` this fully determines the
    # store path below; consumers that rebuild the FOD locally with this
    # hash land at the exact same path.
    outputHash = "sha256-MwbyzXwxWEzOY7Z6ulHMAWYi8Nz2XE8eew/YaP1/SaE=";

    # Realized FOD output path. Consumers that only need a string (e.g. to
    # pass as `macosSdkStorePath` into harbor-rs's `mkCross`) can read this
    # directly without evaluating the SDK derivation, so they never need
    # the source archive locally.
    storePath = "/nix/store/h4wg9712cqahvd47057n9jsqfn9kx389-macosx-sdk-26.1";

    # Private Attic cache that serves the realized SDK. Hosts that pin the
    # `storePath` above must trust this cache to substitute it.
    #
    # PRIVACY: macOS SDKs carry Apple's license. This cache MUST stay
    # private — never expose it anonymously, never mirror it publicly.
    attic = {
      cacheName = "harbor-macos-sdk";
      url = "https://attic.candee.baby/harbor-macos-sdk";
      publicKey = "harbor-macos-sdk:MLRX9qZASKwDh48UWON67cvYxfEbqvjfIZQmGwt1v1E=";
    };
  };
}
