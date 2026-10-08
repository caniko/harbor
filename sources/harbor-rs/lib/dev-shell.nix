# mkDevShell   :: { ... } -> devShell derivation
# mkDocsShell  :: { ... } -> devShell derivation
# mkDevShells  :: { ... } -> { default, windows, macos, cross }
#
# Build devShells with Rust cross-compilation environment variables pre-configured.
# mkDevShells calls mkDevShell internally, so both live in the same file.
#
# The generic shell helpers (mkPkgConfigEnv, mkProjectCliShellTools) live in
# harbor-meta's devShell lib; opencode LSP packages derive from the rust
# profile in ./opencode.nix (rust-analyzer itself is toolchain-provided).
{
  metaDevShell ? null,
  opencodeProfiles ? null,
}: rec {
  # Build a devShell with Rust cross-compilation environment variables pre-configured.
  mkDevShell = {
    pkgs,
    craneLib,
    cross,
    enableWindowsEnv ? true,
    enableOsxcrossEnv ? true,
    pkgConfigDeps ? [],
    packages ? [],
    extraEnv ? {},
    extraShellHook ? "",
    checks ? {},
    cargoConfig ? (craneLib.rsHarborCargoConfig or null),
    opencodeLsp ? {enable = true;},
  }: let
    inherit (cross) mingwBinutils osxcrossToolchain osxcrossRustHelpers;

    basePackages = with pkgs; [
      cargo-audit
      cargo-deny
      cargo-sweep
      cmake
      gcc
      clang
      mold
      lld
      pkg-config
    ];

    windowsPackages = pkgs.lib.optionals enableWindowsEnv [
      mingwBinutils
    ];

    osxPackages = pkgs.lib.optionals (enableOsxcrossEnv && osxcrossToolchain != null) [
      osxcrossToolchain
    ];

    opencodeLspPackages = pkgs.lib.optionals (opencodeLsp.enable or true) (
      if opencodeProfiles == null
      then throw "harbor-rs: mkDevShell opencode LSP packages require the harbor-meta flake input"
      else opencodeProfiles.rust.packages pkgs
    );

    osxShellHook =
      if enableOsxcrossEnv && osxcrossRustHelpers != null
      then osxcrossRustHelpers.mkDevShellHook {}
      else "";

    cargoConfigHook =
      if cargoConfig != null
      then ''
        __rs_harbor_cfg_hash="$(${pkgs.coreutils}/bin/sha256sum ${cargoConfig.configPath} | ${pkgs.coreutils}/bin/cut -c1-16)"
        RS_HARBOR_CARGO_HOME="''${XDG_CACHE_HOME:-$HOME/.cache}/harbor-rs/cargo-config-$__rs_harbor_cfg_hash"
        if [ -z "''${CARGO_HOME:-}" ]; then
          export CARGO_HOME="$RS_HARBOR_CARGO_HOME"
        fi
        if [ "$CARGO_HOME" = "$RS_HARBOR_CARGO_HOME" ]; then
          mkdir -p "$RS_HARBOR_CARGO_HOME" || return 1
          # Concurrent direnv shells must not unlink or expose a partial config.
          __rs_harbor_cfg_tmp="$(mktemp "$RS_HARBOR_CARGO_HOME/.config.toml.XXXXXX")" || return 1
          if ! install -m 0644 ${cargoConfig.configPath} "$__rs_harbor_cfg_tmp" \
            || ! ${pkgs.coreutils}/bin/mv -fT "$__rs_harbor_cfg_tmp" "$RS_HARBOR_CARGO_HOME/config.toml"; then
            rm -f "$__rs_harbor_cfg_tmp"
            return 1
          fi
          echo "harbor-rs: cargo config at $CARGO_HOME/config.toml" >&2
        else
          echo "harbor-rs: keeping existing CARGO_HOME=$CARGO_HOME; generated Cargo config is not activated" >&2
        fi
      ''
      else "";

    baseEnv = {
      LIBCLANG_PATH = pkgs.lib.makeLibraryPath [pkgs.clang.cc];
    };

    crossEnv =
      if enableWindowsEnv
      then cross.windowsEnv
      else {};

    pkgConfigEnv = metaDevShell.mkPkgConfigEnv {
      inherit pkgs;
      deps = pkgConfigDeps;
    };

    mergedEnv = baseEnv // crossEnv // pkgConfigEnv // extraEnv;
  in
    if metaDevShell == null
    then throw "harbor-rs: mkDevShell requires the harbor-meta flake input"
    else
      metaDevShell.mkShell {
        inherit pkgs;
        packages = basePackages ++ windowsPackages ++ osxPackages ++ opencodeLspPackages ++ packages;
        env = mergedEnv;
        extraShellHook = ''
          ${cargoConfigHook}
          ${osxShellHook}
          ${extraShellHook}
        '';
        builder = spec:
          craneLib.devShell (
            spec.env
            // {
              inherit checks;
              inherit (spec) packages shellHook;
            }
          );
      };

  # Build a docs/tooling shell that starts from the same foundation as
  # mkDevShell, but keeps cross-compilation environment variables disabled
  # unless a downstream project explicitly opts back in.
  mkDocsShell = args @ {
    enableWindowsEnv ? false,
    enableOsxcrossEnv ? false,
    ...
  }:
    mkDevShell (args
      // {
        inherit enableWindowsEnv enableOsxcrossEnv;
      });

  # Build multiple devShells for workspace ergonomics.
  #
  # Returns four shells from one config: default (native-only),
  # windows, macos, and cross (all targets). Downstream workspaces
  # use `nix develop` for native and `nix develop .#cross` etc.
  mkDevShells = {
    pkgs,
    craneLib,
    cross,
    enableOsxcrossEnv ? true,
    pkgConfigDeps ? [],
    packages ? [],
    extraEnv ? {},
    extraShellHook ? "",
    checks ? {},
    cargoConfig ? (craneLib.rsHarborCargoConfig or null),
    opencodeLsp ? {enable = true;},
  }: let
    shell = {
      win ? false,
      osx ? false,
    }:
      mkDevShell {
        inherit pkgs craneLib cross pkgConfigDeps packages extraEnv extraShellHook checks cargoConfig opencodeLsp;
        enableWindowsEnv = win;
        enableOsxcrossEnv = osx && enableOsxcrossEnv;
      };
  in {
    default = shell {};
    windows = shell {win = true;};
    macos = shell {osx = true;};
    cross = shell {
      win = true;
      osx = true;
    };
  };
}
