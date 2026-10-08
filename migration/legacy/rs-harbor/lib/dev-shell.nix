# mkDevShell   :: { ... } -> devShell derivation
# mkDocsShell  :: { ... } -> devShell derivation
# mkDevShells  :: { ... } -> { default, windows, macos, cross }
#
# Build devShells with Rust cross-compilation environment variables pre-configured.
# mkDevShells calls mkDevShell internally, so both live in the same file.
rec {
  mkPkgConfigEnv = {
    pkgs,
    deps ? [],
  }:
    pkgs.lib.optionalAttrs (deps != []) {
      PKG_CONFIG_PATH = pkgs.lib.makeSearchPathOutput "dev" "lib/pkgconfig" deps;
    };

  # Build a small reusable package/hook pair for making a project CLI
  # available in dev shells without allowing an older PATH entry to win.
  mkProjectCliShellTools = {
    pkgs,
    package,
    commandName,
    hint ? "",
    versionCheck ? {},
  }: let
    lib = pkgs.lib;
    expectedPath = "${package}/bin/${commandName}";
    expected = versionCheck.expected or null;
    versionCommand = versionCheck.command or "${commandName} --version";
    predicate = versionCheck.predicate or null;
    hintHook = lib.optionalString (hint != "") ''
      echo ${lib.escapeShellArg hint}
    '';
    expectedHook = lib.optionalString (expected != null) ''
      __rs_harbor_project_cli_version="$(${versionCommand} 2>&1 || true)"
      if ! printf '%s\n' "$__rs_harbor_project_cli_version" | grep -F -- ${lib.escapeShellArg expected} >/dev/null; then
        echo "rs-harbor: ${commandName} version check failed; expected output containing ${expected}" >&2
        echo "$__rs_harbor_project_cli_version" >&2
        return 1 2>/dev/null || exit 1
      fi
    '';
    predicateHook = lib.optionalString (predicate != null) ''
      RS_HARBOR_PROJECT_CLI=${lib.escapeShellArg commandName} \
      RS_HARBOR_PROJECT_CLI_PATH="$__rs_harbor_project_cli_resolved" \
        ${predicate}
    '';
  in {
    packages = [package];
    shellHook = ''
      __rs_harbor_project_cli_resolved="$(command -v ${lib.escapeShellArg commandName} || true)"
      if [ -z "$__rs_harbor_project_cli_resolved" ]; then
        echo "rs-harbor: ${commandName} is not available on PATH" >&2
        return 1 2>/dev/null || exit 1
      fi
      if [ "$__rs_harbor_project_cli_resolved" != ${lib.escapeShellArg expectedPath} ]; then
        echo "rs-harbor: ${commandName} resolved to $__rs_harbor_project_cli_resolved, expected ${expectedPath}" >&2
        return 1 2>/dev/null || exit 1
      fi
      ${expectedHook}
      ${predicateHook}
      ${hintHook}
    '';
  };

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

    opencodeLspPackages = pkgs.lib.optionals (opencodeLsp.enable or true) [
      pkgs.nixd
      pkgs.taplo
    ];

    osxShellHook =
      if enableOsxcrossEnv && osxcrossRustHelpers != null
      then osxcrossRustHelpers.mkDevShellHook {}
      else "";

    cargoConfigHook =
      if cargoConfig != null
      then ''
        __rs_harbor_cfg_hash="$(${pkgs.coreutils}/bin/sha256sum ${cargoConfig.configPath} | ${pkgs.coreutils}/bin/cut -c1-16)"
        RS_HARBOR_CARGO_HOME="''${XDG_CACHE_HOME:-$HOME/.cache}/rs-harbor/cargo-config-$__rs_harbor_cfg_hash"
        if [ -z "''${CARGO_HOME:-}" ]; then
          export CARGO_HOME="$RS_HARBOR_CARGO_HOME"
        fi
        mkdir -p "$RS_HARBOR_CARGO_HOME"
        install -m 0644 ${cargoConfig.configPath} "$RS_HARBOR_CARGO_HOME/config.toml"
        echo "rs-harbor: cargo config at $RS_HARBOR_CARGO_HOME/config.toml"
      ''
      else "";

    baseEnv = {
      LIBCLANG_PATH = pkgs.lib.makeLibraryPath [pkgs.clang.cc];
    };

    crossEnv =
      if enableWindowsEnv
      then cross.windowsEnv
      else {};

    pkgConfigEnv = mkPkgConfigEnv {
      inherit pkgs;
      deps = pkgConfigDeps;
    };

    mergedEnv = baseEnv // crossEnv // pkgConfigEnv // extraEnv;
  in
    craneLib.devShell (mergedEnv
      // {
        inherit checks;

        packages = basePackages ++ windowsPackages ++ osxPackages ++ opencodeLspPackages ++ packages;

        shellHook = ''
          ${cargoConfigHook}
          ${osxShellHook}
          ${extraShellHook}
        '';
      });

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
