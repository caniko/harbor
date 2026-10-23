# mkDevShell   :: { ... } -> devShell derivation
# mkDocsShell  :: { ... } -> devShell derivation
# mkDevShells  :: { ... } -> { default, windows, macos, cross }
# mkToolchainShell :: { pkgs, toolchainProfile?, targets?, ... } -> devShell derivation
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
  mkToolchain ? null,
  rustOverlay ? null,
}: rec {
  # Install a generated Cargo config as CARGO_HOME. An inherited CARGO_HOME is
  # kept unless it is another harbor-rs generated config, whose channel or
  # targets need not match this shell (stable cargo rejects nightly -Z flags).
  mkCargoHomeHook = {
    pkgs,
    cargoConfig,
  }:
    if cargoConfig != null
    then ''
      __rs_harbor_cfg_hash="$(${pkgs.coreutils}/bin/sha256sum ${cargoConfig.configPath} | ${pkgs.coreutils}/bin/cut -c1-16)"
      RS_HARBOR_CARGO_HOME="''${XDG_CACHE_HOME:-$HOME/.cache}/harbor-rs/cargo-config-$__rs_harbor_cfg_hash"
      case "''${CARGO_HOME:-}" in
        "" | "''${XDG_CACHE_HOME:-$HOME/.cache}"/harbor-rs/cargo-config-*)
          export CARGO_HOME="$RS_HARBOR_CARGO_HOME"
          ;;
      esac
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

  # A composable Rust toolchain shell for polyglot projects: no crane, cross,
  # MinGW or osxcross. Pass it to harbor-meta's `devShell.mkShell { shells }`.
  # Defaults to the pinned stable profile with only the host standard library.
  mkToolchainShell = {
    pkgs,
    timeZone ? "UTC",
    toolchainProfile ? "stable",
    targets ? [],
    packages ? [],
    extraEnv ? {},
    extraShellHook ? "",
    opencodeLsp ? {enable = true;},
  }: let
    pkgs' =
      if pkgs ? rust-bin
      then pkgs
      else if rustOverlay == null
      then throw "harbor-rs: mkToolchainShell requires the rust-overlay flake input"
      else pkgs.extend rustOverlay.overlays.default;
    toolchain = mkToolchain {
      pkgs = pkgs';
      inherit toolchainProfile targets;
    };
    opencodeLspPackages = pkgs.lib.optionals (opencodeLsp.enable or true) (
      if opencodeProfiles == null
      then throw "harbor-rs: mkToolchainShell opencode LSP packages require the harbor-meta flake input"
      else opencodeProfiles.rust.packages pkgs'
    );
    shell = metaDevShell.mkShell {
      pkgs = pkgs';
      inherit timeZone;
      packages = [toolchain.rustToolchain] ++ opencodeLspPackages ++ packages;
      env = extraEnv;
      extraShellHook = ''
        ${mkCargoHomeHook {
          pkgs = pkgs';
          inherit (toolchain) cargoConfig;
        }}
        ${extraShellHook}
      '';
    };
  in
    if metaDevShell == null || mkToolchain == null
    then throw "harbor-rs: mkToolchainShell requires the harbor-meta flake input"
    else shell // {passthru = shell.passthru // {inherit toolchain;};};

  # Build a devShell with Rust cross-compilation environment variables pre-configured.
  mkDevShell = {
    pkgs,
    timeZone ? "UTC",
    craneLib,
    cross,
    enableWindowsEnv ? true,
    enableOsxcrossEnv ? true,
    pkgConfigDeps ? [],
    packages ? [],
    extraEnv ? {},
    extraShellHook ? "",
    runtime ? null,
    nativeRunner ? [],
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

    cargoConfigHook = mkCargoHomeHook {inherit pkgs cargoConfig;};

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

    native = import ./native-runner.nix {inherit pkgs runtime nativeRunner;};
    runnerConflict = builtins.any (name: builtins.hasAttr name extraEnv) (builtins.attrNames native.env);
    mergedEnv = baseEnv // crossEnv // pkgConfigEnv // native.env // extraEnv;
  in
    assert pkgs.lib.assertMsg (!runnerConflict) "harbor-rs: pass an existing native runner through nativeRunner when configuring runtime";
      if metaDevShell == null
      then throw "harbor-rs: mkDevShell requires the harbor-meta flake input"
      else
        metaDevShell.mkShell {
          inherit pkgs timeZone;
          packages = basePackages ++ windowsPackages ++ osxPackages ++ opencodeLspPackages ++ native.packages ++ packages;
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
                inherit (spec) packages shellHook inputsFrom;
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
    timeZone ? "UTC",
    craneLib,
    cross,
    enableOsxcrossEnv ? true,
    pkgConfigDeps ? [],
    packages ? [],
    extraEnv ? {},
    extraShellHook ? "",
    runtime ? null,
    nativeRunner ? [],
    checks ? {},
    cargoConfig ? (craneLib.rsHarborCargoConfig or null),
    opencodeLsp ? {enable = true;},
  }: let
    shell = {
      win ? false,
      osx ? false,
    }:
      mkDevShell {
        inherit pkgs timeZone craneLib cross pkgConfigDeps packages extraEnv extraShellHook checks cargoConfig opencodeLsp runtime nativeRunner;
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
