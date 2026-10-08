/*
Pure consumer-shape NixOS module for harbor-rs macOS SDK
cross-compilation.

Producer workflow (operator-driven, not declarative):
  nix run harbor-rs#publish-macos-sdk -- \
    --archive /path/to/MacOSX.sdk.tar.xz \
    --version 26.1 \
    --attic-server https://attic.candee.baby \
    --cache harbor-macos-sdk \
    --token-file /run/agenix/attic-harbor-macos-sdk-token \
    --gc-root /nix/var/nix/gcroots/rs-harbor-macos-sdk-26.1

Commit the printed storePath into this module on every host
that wants the realized SDK; consumers substitute from the
configured Attic cache.

Hosts that bind-mount or build against the realized SDK (e.g. CI
runners) should set `keepRealized = true` so a periodic
`nix-collect-garbage` can never silently evict the pinned store
path out from under the bind-mount. The `--gc-root` flag above is
the imperative equivalent for the host that publishes the SDK.
*/
{
  config,
  lib,
  ...
}: let
  cfg = config.programs.harborRs.macosSdk;
in {
  options.programs.harborRs.macosSdk = {
    enable = lib.mkEnableOption "harbor-rs macOS SDK realization for osxcross cross-compilation";

    sdkVersion = lib.mkOption {
      type = lib.types.str;
      default = "26.1";
      description = "macOS SDK version used when pinning the realized SDK store path.";
    };

    storePath = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = ''
        Realized macOS SDK store path injected into harbor-rs project wrappers
        via `mkCrossArgs`. Produce it with `nix run harbor-rs#publish-macos-sdk
        -- ...` from any host, then commit the printed store path into the
        consuming NixOS configuration.
      '';
    };

    outputHash = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = ''
        Recursive fixed-output hash for `storePath`. Set this alongside
        `storePath` so `mkCrossArgs` can reconstruct the SDK as a
        context-carrying build input for sandboxed osxcross builds.
      '';
    };

    keepRealized = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = ''
        Pin the realized macOS SDK store path as a Nix GC root on this host
        so it survives `nix-collect-garbage`. Enable on hosts that bind-mount
        or cross-build against the SDK (e.g. CI runners) so a periodic GC can
        never silently evict the path the build resolves against. The path
        must already be present in this host's store (substituted from the
        Attic cache or produced by the publish workflow). No-op unless
        `storePath` is set.
      '';
    };

    mkCrossArgs = lib.mkOption {
      type = lib.types.attrs;
      readOnly = true;
      default =
        {osxSdkVersion = cfg.sdkVersion;}
        // lib.optionalAttrs (cfg.storePath != null) {
          macosSdkStorePath = cfg.storePath;
        }
        // lib.optionalAttrs (cfg.storePath != null && cfg.outputHash != null) {
          macosSdkOutputHash = cfg.outputHash;
        };
      defaultText = lib.literalExpression ''
        {
          osxSdkVersion = config.programs.harborRs.macosSdk.sdkVersion;
        } // optional macosSdkStorePath and macosSdkOutputHash
      '';
      description = "Arguments to merge into harbor-rs.lib.mkCross from project wrappers.";
    };
  };

  config = lib.mkIf (cfg.enable && cfg.keepRealized && cfg.storePath != null) {
    # A symlink under /nix/var/nix/gcroots that points into the store is a
    # direct GC root, so the realized SDK can never be silently collected.
    # `L+` force-replaces a stale link if the pinned storePath changes.
    systemd.tmpfiles.rules = [
      "L+ /nix/var/nix/gcroots/rs-harbor-macos-sdk-${cfg.sdkVersion} - - - - ${cfg.storePath}"
    ];
  };
}
