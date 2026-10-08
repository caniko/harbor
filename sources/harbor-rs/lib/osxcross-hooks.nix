# mkOsxcrossHooks :: { llvmPackages } -> { appleClangShimsHook, macosShellGuard }
#
# Shell-hook fragments for dev shells that cross-compile to Apple targets
# via osxcross.
#
# - appleClangShimsHook: when OSXCROSS_TARGET_DIR is set, prepends a tempdir
#   of clang/clang++/cc/c++/dsymutil/xcode-select shims to PATH. The clang
#   shim dispatches to CC_<target>/CXX_<target> based on TARGET, --target=,
#   or -arch detection so build scripts that invoke a bare `clang` still
#   route to the correct osxcross-aware compiler. dsymutil falls back to
#   llvm-dsymutil; xcode-select --print-path returns OSXCROSS_TARGET_DIR.
#
# - macosShellGuard: bails out of the shell if OSXCROSS_SDKROOT or the
#   per-target cargo linkers are missing — protects against entering the
#   .#macos/.#cross shells without --impure when osxcross relies on env
#   vars to locate the SDK store path.
{llvmPackages}: {
  appleClangShimsHook = ''
        if [ -n "''${OSXCROSS_TARGET_DIR:-}" ]; then
          osxcross_shims="$(mktemp -d /tmp/osxcross-shims.XXXXXX)"
          export RS_HARBOR_HOST_CLANG="$(type -P clang || true)"
          export RS_HARBOR_HOST_CLANGXX="$(type -P clang++ || true)"

          cat > "$osxcross_shims/apple-clang-dispatch" <<'EOF'
    #!/usr/bin/env nu

    const self_path = (path self)

    def detect-apple-arch [args: list<string>] {
      for target in [
        ($env.TARGET? | default ""),
        ($env.CARGO_BUILD_TARGET? | default ""),
      ] {
        if ($target | str starts-with "x86_64-apple-darwin") {
          return "x86_64"
        }

        if (($target | str starts-with "aarch64-apple-darwin") or ($target | str starts-with "arm64-apple-darwin")) {
          return "arm64"
        }
      }

      mut expect_arch = false
      for arg in $args {
        if $expect_arch {
          if $arg == "x86_64" {
            return "x86_64"
          }

          if ($arg == "arm64" or $arg == "aarch64") {
            return "arm64"
          }

          $expect_arch = false
          continue
        }

        if $arg == "-arch" {
          $expect_arch = true
          continue
        }

        if ($arg | str starts-with "--target=x86_64-apple-darwin") {
          return "x86_64"
        }

        if (($arg | str starts-with "--target=aarch64-apple-darwin") or ($arg | str starts-with "--target=arm64-apple-darwin")) {
          return "arm64"
        }
      }

      null
    }

    def --wrapped main [...args: string] {
      let tool = ($self_path | path basename)
      let config = if ($tool == "clang" or $tool == "cc") {
        {
          x86: ($env.CC_x86_64_apple_darwin? | default ""),
          arm: ($env.CC_aarch64_apple_darwin? | default ""),
          fallback: ($env.RS_HARBOR_HOST_CLANG? | default "clang"),
        }
      } else if ($tool == "clang++" or $tool == "c++") {
        {
          x86: ($env.CXX_x86_64_apple_darwin? | default ""),
          arm: ($env.CXX_aarch64_apple_darwin? | default ""),
          fallback: ($env.RS_HARBOR_HOST_CLANGXX? | default "clang++"),
        }
      } else {
        print --stderr $"unsupported apple clang shim: ($tool)"
        exit 64
      }

      let arch = (detect-apple-arch $args)

      if ($arch == "x86_64" and (($config.x86 | str trim) | is-not-empty)) {
        ^$config.x86 ...$args
        return
      }

      if ($arch == "arm64" and (($config.arm | str trim) | is-not-empty)) {
        ^$config.arm ...$args
        return
      }

      ^$config.fallback ...$args
    }
    EOF
          chmod +x "$osxcross_shims/apple-clang-dispatch"
          ln -s apple-clang-dispatch "$osxcross_shims/clang"
          ln -s apple-clang-dispatch "$osxcross_shims/clang++"
          ln -s apple-clang-dispatch "$osxcross_shims/cc"
          ln -s apple-clang-dispatch "$osxcross_shims/c++"

          cat > "$osxcross_shims/dsymutil" <<'EOF'
    #!/usr/bin/env nu

    def --wrapped main [...args: string] {
      ^"${llvmPackages.llvm}/bin/dsymutil" ...$args
    }
    EOF
          chmod +x "$osxcross_shims/dsymutil"

          if command -v xcrun >/dev/null && ! command -v xcode-select >/dev/null; then
          cat > "$osxcross_shims/xcode-select" <<'EOF'
    #!/usr/bin/env nu

    def --wrapped main [...args: string] {
      if (($args | length) == 1 and ($args | get 0) == "--print-path") {
        print ($env.OSXCROSS_TARGET_DIR? | default ($env.OSXCROSS_SDKROOT? | default "/"))
        return
      }

      print --stderr "xcode-select shim only supports --print-path"
      exit 1
    }
    EOF
          chmod +x "$osxcross_shims/xcode-select"
          fi
          export PATH="$osxcross_shims:$PATH"
        fi
  '';

  macosShellGuard = ''
    missing_macos_toolchain=0
    for var_name in \
      OSXCROSS_SDKROOT \
      CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER \
      CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER
    do
      var_value="''${!var_name:-}"
      if [ -z "$var_value" ]; then
        printf '%s\n' "Missing macOS cross-compilation environment variable: $var_name" >&2
        missing_macos_toolchain=1
      fi
    done

    if [ "$missing_macos_toolchain" -ne 0 ]; then
      printf '%s\n' "The .#macos/.#cross shell did not finish wiring osxcross. Re-enter via nix develop --impure .#macos or nix develop --impure .#cross." >&2
      exit 1
    fi
  '';
}
