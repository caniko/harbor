# mkSteamRuntimeTools :: { pkgs, rsHarborCli, runtime?, customImage?, containerRuntime?, steamworksRsLibSubdir? } -> attrs
#
# Steam Linux Runtime helpers. Returns runtime metadata, default
# DT_NEEDED/DLL/dylib allowlists for sniper-targeted builds, a shellHook
# that exposes the Steamworks SDK redistributable libs from a `steamworks-rs`
# cargo git checkout, and writeShellApplication shims around the harbor-rs
# Rust CLI that bake in the selected runtime/image.
#
# `rsHarborCli` (the harbor-rs CLI derivation, e.g.
# `harbor-rs.packages.${system}.harbor-rs`) is required.
{
  pkgs,
  rsHarborCli,
  runtime ? "sniper",
  customImage ? null,
  containerRuntime ? "podman",
  steamworksRsLibSubdir ? "linux64",
}: let
  inherit (pkgs) lib;

  runtimes = {
    sniper = {
      name = "Steam Linux Runtime 3.0 (sniper)";
      sdkImage = "registry.gitlab.steamos.cloud/steamrt/sniper/sdk";
      platformImage = "registry.gitlab.steamos.cloud/steamrt/sniper/platform";
      steamworksToolAppId = "1628350";
    };
    scout = {
      name = "Steam Linux Runtime 1.0 (scout)";
      sdkImage = "registry.gitlab.steamos.cloud/steamrt/scout/sdk";
      platformImage = "registry.gitlab.steamos.cloud/steamrt/scout/platform";
      steamworksToolAppId = "1070560";
    };
  };

  selectedRuntime =
    if lib.hasAttr runtime runtimes
    then runtimes.${runtime}
    else {
      name = runtime;
      sdkImage = runtime;
      platformImage = runtime;
      steamworksToolAppId = null;
    };

  image =
    if customImage != null
    then customImage
    else selectedRuntime.sdkImage;

  containerCommandText = ''
    ${containerRuntime} run --rm -it \
      --userns=keep-id \
      --volume "$PWD:$PWD" \
      --workdir "$PWD" \
      --user "$(id -u):$(id -g)" \
      ${image} -- <command>
  '';

  # Shell shim wrapping `harbor-rs steam-runtime exec` with the selected
  # runtime / image / container runtime baked in. The shim still accepts
  # the old `--runtime`, `--image`, `--container-runtime` overrides since
  # clap re-parses everything passed after the baked defaults.
  steamRuntimeExec = pkgs.writeShellApplication {
    name = "steam-runtime-exec";
    runtimeInputs = [rsHarborCli];
    text = ''
      exec harbor-rs steam-runtime exec \
        --runtime ${runtime} \
        --image ${image} \
        --container-runtime ${containerRuntime} \
        "$@"
    '';
  };

  # Allowlist regexes for binaries shipped to Steam alongside the sniper
  # runtime. They cover the system libraries that ship in sniper, the
  # standard Win32 DLLs supplied by the Windows runtime/loader, and the
  # macOS framework search roots permitted for redistributable bundles.
  # `libsteam_api`, `steam_api64.dll`, and `libsteam_api.dylib` are the
  # canonical Steamworks SDK shared libraries and are included so projects
  # that link against `steamworks-rs` pass without further configuration.
  defaultAllowRegexes = {
    linuxNeeded = "^(ld-linux.*|lib(c|m|dl|rt|pthread|gcc_s|stdc\\+\\+|steam_api|vulkan|X11|Xi|Xcursor|Xrandr|Xinerama|Xfixes|xcb|wayland-client|xkbcommon|asound|udev|GL|EGL|drm|gbm|expat|z|bz2|fontconfig|freetype|png16|zstd|graphite2|harfbuzz|brotli.*|dbus-1|systemd|cap|resolv|util|nss_.*|nsl|anl).*)\\.so(\\..*)?$";
    windowsDll = "^(ADVAPI32|BCRYPT|bcryptprimitives|combase|CRYPT32|DBGHELP|DNSAPI|dwmapi|GDI32|imm32|IPHLPAPI|KERNEL32|MSVCP140|NTDLL|OLE32|OLEAUT32|OPENGL32|POWRPROF|secur32|SHELL32|uiautomationcore|USER32|USERENV|uxtheme|VCRUNTIME140|VCRUNTIME140_1|VERSION|WINHTTP|WINMM|WS2_32|api-ms-win-.*|steam_api64)\\.dll$";
    macosDylib = "^(@executable_path|@loader_path/libsteam_api\\.dylib$|@rpath|/usr/lib/|/System/Library/)";
  };

  # Cargo bootstrap script intended to run *inside* a Steam Runtime sniper
  # SDK container (or invoked via steam-runtime-exec). Provisions a hermetic
  # rustup install under STEAM_RUNTIME_BUILD_ROOT, then runs cargo build for
  # a single target. Kept as a portable shell script because the sniper
  # Debian-based container has no Rust toolchain to invoke a Rust binary
  # against.
  steamRuntimeCargoBootstrap = pkgs.writeShellApplication {
    name = "steam-runtime-cargo-bootstrap";
    runtimeInputs = with pkgs; [coreutils curl];
    text = ''
      runtime_root="''${STEAM_RUNTIME_BUILD_ROOT:-$PWD/target/steam-runtime}"
      toolchain="''${STEAM_RUNTIME_RUST_TOOLCHAIN:-stable}"
      target="''${STEAM_RUNTIME_TARGET:-x86_64-unknown-linux-gnu}"
      features=""
      profile="release"
      cargo_args=()

      usage() {
        cat <<'USAGE'
      Usage: steam-runtime-cargo-bootstrap [options]

      Provisions a per-runtime rustup toolchain under STEAM_RUNTIME_BUILD_ROOT
      and runs `cargo build`. Designed to execute inside the Steam Runtime
      sniper SDK container (intentionally avoids touching the user's host
      cargo/rustup state).

      Environment:
        STEAM_RUNTIME_BUILD_ROOT     Root for HOME/CARGO_HOME/RUSTUP_HOME
                                     (default: $PWD/target/steam-runtime)
        STEAM_RUNTIME_RUST_TOOLCHAIN Rust toolchain channel (default: stable)
        STEAM_RUNTIME_TARGET         Cargo target triple
                                     (default: x86_64-unknown-linux-gnu)
        LIBCLANG_PATH                Optional, defaults to
                                     /usr/lib/x86_64-linux-gnu
        CARGO_PROFILE_RELEASE_DEBUG  Forwarded; defaults to 1

      Options:
        --features LIST     Cargo --features value
        --profile NAME      Cargo profile (default: release)
        --                  Forward remaining args to cargo build
        -h, --help          Show this help
      USAGE
      }

      while [ "$#" -gt 0 ]; do
        case "$1" in
          --features) features="$2"; shift 2 ;;
          --profile) profile="$2"; shift 2 ;;
          --) shift; cargo_args+=("$@"); break ;;
          -h|--help) usage; exit 0 ;;
          *) echo "steam-runtime-cargo-bootstrap: unknown option: $1" >&2; usage >&2; exit 2 ;;
        esac
      done

      export HOME="$runtime_root/home"
      export CARGO_HOME="$runtime_root/cargo"
      export RUSTUP_HOME="$runtime_root/rustup"
      export CARGO_TARGET_DIR="''${CARGO_TARGET_DIR:-$runtime_root/target}"
      export LIBCLANG_PATH="''${LIBCLANG_PATH:-/usr/lib/x86_64-linux-gnu}"
      export CARGO_PROFILE_RELEASE_DEBUG="''${CARGO_PROFILE_RELEASE_DEBUG:-1}"

      mkdir -p "$HOME" "$CARGO_HOME" "$RUSTUP_HOME" "$CARGO_TARGET_DIR"

      for tool in curl git gcc g++ pkg-config clang; do
        if ! command -v "$tool" >/dev/null 2>&1; then
          echo "steam-runtime-cargo-bootstrap: missing required tool in sniper SDK: $tool" >&2
          exit 127
        fi
      done

      export PATH="$CARGO_HOME/bin:$PATH"

      if ! command -v rustup >/dev/null 2>&1; then
        rustup_init="$runtime_root/rustup-init.sh"
        curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o "$rustup_init"
        sh "$rustup_init" -y --no-modify-path --profile minimal --default-toolchain "$toolchain"
      fi

      rustup toolchain install "$toolchain" --profile minimal

      build_args=(+"$toolchain" build "--profile=$profile" "--target" "$target")
      if [ -n "$features" ]; then
        build_args+=(--features "$features")
      fi
      build_args+=("''${cargo_args[@]}")

      exec cargo "''${build_args[@]}"
    '';
  };

  # Shell hook fragment that locates the Steamworks SDK shared libraries
  # vendored inside a `steamworks-rs` cargo git checkout and prepends
  # the matching arch subdirectory (linux64 / linux32 / osx) to
  # LD_LIBRARY_PATH so cargo-built binaries can dlopen libsteam_api.
  steamworksRsCargoLibraryHook = ''
    # Steamworks native library (from cargo git checkout)
    for dir in "$HOME"/.cargo/git/checkouts/steamworks-rs-*/*/steamworks-sys/lib/steam/redistributable_bin/${steamworksRsLibSubdir}; do
      if [ -d "$dir" ]; then
        export LD_LIBRARY_PATH="$dir:$LD_LIBRARY_PATH"
        break
      fi
    done
  '';

  # Audit helpers: thin shims around `harbor-rs audit elf|pe|macho`. The
  # rust implementation parses ELF/PE/Mach-O via goblin, has unit and
  # integration tests, and uses clap for argument parsing.
  rustAuditShim = {
    name,
    subcommand,
  }:
    pkgs.writeShellApplication {
      inherit name;
      runtimeInputs = [rsHarborCli];
      text = ''
        exec harbor-rs audit ${subcommand} "$@"
      '';
    };

  auditElfRuntimeDeps = rustAuditShim {
    name = "audit-elf-runtime-deps";
    subcommand = "elf";
  };
  auditWindowsRuntimeDeps = rustAuditShim {
    name = "audit-windows-runtime-deps";
    subcommand = "pe";
  };
  auditDarwinRuntimeDeps = rustAuditShim {
    name = "audit-darwin-runtime-deps";
    subcommand = "macho";
  };
in {
  inherit runtimes selectedRuntime image containerCommandText;
  inherit steamRuntimeExec auditElfRuntimeDeps auditWindowsRuntimeDeps auditDarwinRuntimeDeps;
  inherit defaultAllowRegexes steamworksRsCargoLibraryHook steamRuntimeCargoBootstrap;

  packages = {
    inherit
      steamRuntimeExec
      auditElfRuntimeDeps
      auditWindowsRuntimeDeps
      auditDarwinRuntimeDeps
      steamRuntimeCargoBootstrap
      ;
  };
}
