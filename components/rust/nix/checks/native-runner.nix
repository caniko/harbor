{
  pkgs,
  metaDevShell,
}: let
  runtime = metaDevShell.mkRuntime {
    inherit pkgs;
    env.HARBOR_RUNTIME_PROBE = "scoped";
  };
  custom = pkgs.writeShellScript "custom-native-runner" ''
    test "$1" = "argument with spaces"
    shift
    test "$HARBOR_RUNTIME_PROBE" = scoped
    echo called >> "$HARBOR_RUNNER_LOG"
    exec "$@"
  '';
  native = import ../../lib/native-runner.nix {
    inherit pkgs runtime;
    nativeRunner = [(toString custom) "argument with spaces"];
  };
  empty = import ../../lib/native-runner.nix {inherit pkgs;};
  devShells = import ../../lib/dev-shell.nix {inherit metaDevShell;};
  shellArgs = {
    inherit pkgs runtime;
    nativeRunner = [(toString custom) "argument with spaces"];
    craneLib.devShell = pkgs.mkShell;
    cross = {
      mingwBinutils = pkgs.emptyDirectory;
      osxcrossToolchain = null;
      osxcrossRustHelpers = null;
      windowsEnv.CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER = "/preserved-cross-runner";
    };
    opencodeLsp.enable = false;
    enableOsxcrossEnv = false;
  };
  shell = devShells.mkDevShell shellArgs;
  variable = builtins.head (builtins.attrNames native.env);
  conflict = builtins.tryEval ((devShells.mkDevShell (shellArgs // {extraEnv.${variable} = "/conflicting-runner";})).drvPath);
in
  assert empty.env == {};
  assert shell.${variable} == native.env.${variable};
  assert shell.CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER == "/preserved-cross-runner";
  assert !conflict.success;
  assert builtins.attrNames native.env == ["CARGO_TARGET_${pkgs.lib.toUpper (builtins.replaceStrings ["-"] ["_"] pkgs.stdenv.hostPlatform.rust.rustcTarget)}_RUNNER"];
    pkgs.runCommand "harbor-rs-native-runner" {
      nativeBuildInputs = [pkgs.cargo pkgs.rustc pkgs.stdenv.cc];
      env = native.env;
    } ''
      export HOME="$TMPDIR/home" CARGO_HOME="$TMPDIR/cargo"
      export HARBOR_RUNNER_LOG="$TMPDIR/runner.log"
      mkdir -p "$HOME" project/src
      cd project
      cat > Cargo.toml <<'TOML'
      [package]
      name = "runtime-probe"
      version = "0.1.0"
      edition = "2021"
      [[bench]]
      name = "probe"
      path = "src/main.rs"
      harness = false
      TOML
      cat > src/main.rs <<'RS'
      fn main() { assert_eq!(std::env::var("HARBOR_RUNTIME_PROBE").unwrap(), "scoped"); }
      #[test]
      fn runtime_is_scoped() { main(); }
      RS
      unset HARBOR_RUNTIME_PROBE RUSTC_WRAPPER RUSTFLAGS
      cargo run --offline --quiet
      cargo test --offline --quiet
      cargo bench --offline --quiet --bench probe
      test "$(wc -l < "$HARBOR_RUNNER_LOG")" = 3
      test -z "''${HARBOR_RUNTIME_PROBE:-}"
      touch "$out"
    ''
