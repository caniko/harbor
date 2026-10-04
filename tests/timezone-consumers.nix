# Source-level API/forwarding regression; no derivations or flake evaluation.
{sources}: let
  lib = rec {
    concatStringsSep = builtins.concatStringsSep;
    mapAttrs = builtins.mapAttrs;
    optional = enabled: value:
      if enabled
      then [value]
      else [];
    optionals = enabled: values:
      if enabled
      then values
      else [];
    optionalString = enabled: value:
      if enabled
      then value
      else "";
    optionalAttrs = enabled: value:
      if enabled
      then value
      else {};
    makeLibraryPath = values: concatStringsSep ":" (map (value: "${value}/lib") values);
    assertMsg = valid: message:
      if valid
      then true
      else throw message;
    hasPrefix = prefix: value: builtins.substring 0 (builtins.stringLength prefix) value == prefix;
    hasInfix = infix: value: builtins.length (builtins.split infix value) > 1;
    splitString = separator: value: builtins.filter builtins.isString (builtins.split separator value);
    all = builtins.all;
    isList = builtins.isList;
    isString = builtins.isString;
    escapeShellArg = value: "'${builtins.replaceStrings ["'"] ["'\\''"] (toString value)}'";
  };
  pkgs =
    {
      inherit lib;
      buildPackages = {
        tzdata = "/fixture/tzdata";
        coreutils = "/fixture/coreutils";
      };
      mkShell = args: args;
      runCommand = name: attrs: script:
        attrs
        // (attrs.passthru or {})
        // {
          inherit script;
          outPath = "/fixture/${name}";
        };
      stdenv.hostPlatform.system = "x86_64-linux";
      stdenv.cc = "/fixture/cc";
    }
    // builtins.listToAttrs (map (name: {
        inherit name;
        value = "/fixture/${name}";
      }) [
        "cargo-audit"
        "cargo-deny"
        "cargo-sweep"
        "cmake"
        "gcc"
        "clang"
        "mold"
        "lld"
        "pkg-config"
        "binutils"
        "coreutils"
        "findutils"
        "gnugrep"
        "gnutar"
        "gzip"
        "zip"
        "file"
        "gawk"
        "jq"
        "cargo-ndk"
        "jdk21"
        "gradle"
        "uv"
        "rustup"
      ])
    // {clang.cc = "/fixture/clang";};
  timezone = import (sources.meta + "/lib/timezone.nix");
  devShell = import (sources.meta + "/lib/shell.nix") {};
  meta = {inherit timezone devShell;};
  metaFlake.lib = meta;
  fixtureShell = attrs: attrs // {overrideAttrs = update: fixtureShell (attrs // update attrs);};
  rustFacade = import (sources.rs + "/lib/default.nix") {
    crane = {};
    osxcross = {};
    harbor-meta = meta;
    harbor-android.mkAndroidDevShell = {
      pkgs,
      androidSdk,
      ndkVersion,
    }:
      fixtureShell {env.TZ = "UTC";};
  };
  zone = "Europe/Istanbul";
  check = shell: assert shell.env.TZ == zone; assert shell.env.TZDIR == "/fixture/tzdata/share/zoneinfo"; true;
  rust = import (sources.rs + "/lib/dev-shell.nix") {metaDevShell = devShell;};
  rustArgs = {
    inherit pkgs;
    timeZone = zone;
    craneLib.devShell = attrs: {env = attrs;};
    cross = {
      mingwBinutils = null;
      osxcrossToolchain = null;
      osxcrossRustHelpers = null;
      windowsEnv = {};
    };
    opencodeLsp.enable = false;
  };
  python = import (sources.py + "/lib/python.nix") {
    nixLib = lib;
    pyproject-nix = {};
    uv2nix = {};
    pyproject-build-systems = {};
    metaDevShell = devShell;
  };
  node = import (sources.js + "/lib/node.nix") {
    inherit lib;
    metaDevShell = devShell;
  };
  bun = import (sources.js + "/lib/bun.nix") {
    inherit lib;
    bun-overlay = {};
    metaDevShell = devShell;
  };
  tex = import (sources.tex + "/lib/shell.nix") {
    nixLib = lib;
    mkTexlive = _: "/fixture/texlive";
    metaDevShell = devShell;
  };
  go = import (sources.go + "/lib/default.nix") {harbor-meta = metaFlake;};
  eth = import (sources.eth + "/lib/default.nix") {
    nixpkgs = {};
    harbor-meta = metaFlake;
  };
  sol = import (sources.sol + "/lib/default.nix") {
    harbor-meta = metaFlake;
    harbor-rs.inputs.rust-overlay = _: {};
  };
  android = import (sources.android + "/lib/android-dev-shell.nix") {harbor-meta = meta;};
  projects = import (sources.projects + "/lib/docs.nix") {
    packageTests = {};
    metaDevShell = devShell;
  };
  ntt = import (sources.ntt + "/lib/default.nix") {harbor-meta = metaFlake;};
  msvc = import (sources.rs + "/lib/windows-msvc-shell.nix") {metaShellTools = devShell;};
  archives = import (sources.rs + "/lib/release-artifacts.nix") {inherit pkgs timezone;};
  portable = import (sources.rs + "/lib/portable-release.nix") {
    inherit pkgs timezone;
    bundlers = {};
  };
  binary = import (sources.rs + "/lib/binary-release.nix") {inherit pkgs timezone;};
  archive = archives.mkReleaseArchive {
    pname = "fixture";
    version = "1.0.0";
    name = "fixture.zip";
    package = "/fixture/package";
    entries = {};
    format = "zip";
    archiveTimezone = zone;
  };
  releaseArgs = {
    pname = "fixture";
    version = "1.0.0";
    archiveTimezone = zone;
    artifacts.x86_64-linux = {
      package = "/fixture/package";
      rustTarget = "x86_64-unknown-linux-musl";
      binaries = ["fixture"];
      entries.fixture.package = "/fixture/package";
      bundler = _: "/fixture/bundled-executable";
    };
  };
  checkArchive = value: assert value.TZ == zone; assert value.TZDIR == "/fixture/tzdata/share/zoneinfo"; true;
in {
  rustAndroidFacade = check (rustFacade.mkAndroidDevShell {
    inherit pkgs;
    timeZone = zone;
    androidSdk = "/fixture/android";
    ndkVersion = "1";
  });
  shellAdapter = let
    original = fixtureShell {
      env.TZ = "UTC";
      passthru.devShellSpec = {
        env.TZ = "UTC";
        shellHook = "";
      };
    };
    adapted = timezone.withShell {
      inherit pkgs;
      shell = original;
      timeZone = zone;
    };
    inherited = timezone.withShell {
      inherit pkgs;
      shell = adapted;
    };
  in
    assert inherited.passthru.devShellSpec.env.TZ == zone; check inherited;
  defaultTimezone = (devShell.mkShell {inherit pkgs;}).env.TZ == "UTC";
  envOverride = check (devShell.mkShell {
    inherit pkgs;
    env.TZ = zone;
  });
  databaseOverride =
    (devShell.mkShell {
      inherit pkgs;
      env.TZDIR = "/custom/zoneinfo";
    }).env.TZDIR
    == "/custom/zoneinfo";
  rust = check (rust.mkDevShell rustArgs);
  rustVariants = builtins.all check (builtins.attrValues (rust.mkDevShells rustArgs));
  python = check (python.mkUvDevShell {
    inherit pkgs;
    timeZone = zone;
    python = "/fixture/python";
    autoSync = false;
    opencodeLsp.enable = false;
  });
  node = check (node.mkNodeDevShell {
    inherit pkgs;
    timeZone = zone;
    nodejs = "/fixture/node";
    pnpm = {version = "10";};
  });
  bun = check (bun.mkBunDevShell {
    inherit pkgs;
    timeZone = zone;
    version = "1.0.0";
  });
  ntt = check (ntt.mkNttDevShell {
    inherit pkgs;
    timeZone = zone;
    pins = {};
    anchorRustToolchain = "/fixture/rust";
  });
  msvc = check (msvc {
    inherit lib;
    pkgs = pkgs // {windows.sdk = "/fixture/sdk";};
    timeZone = zone;
    llvmPackages = {
      clang-unwrapped = "/fixture/clang";
      bintools-unwrapped = "/fixture/lld";
    };
    toolchain.craneLib.devShell = attrs: {env = attrs;};
  });
  tex = check (tex.mkTexDevShell {
    inherit pkgs;
    timeZone = zone;
  });
  go = check (go.mkGoDevShell {
    inherit pkgs;
    timeZone = zone;
    toolchain = {
      go = "go";
      gopls = "gopls";
      golangciLint = "lint";
    };
  });
  eth = check (eth.mkEthDevShell {
    inherit pkgs;
    timeZone = zone;
    foundry = {
      version = "1";
      outPath = "foundry";
    };
    solc = {
      version = "1";
      outPath = "solc";
    };
  });
  sol = check (sol.mkSolanaDevShell {
    inherit pkgs;
    timeZone = zone;
    anchor = "anchor";
    solana = {version = "1";};
    cargoBuildSbf = "sbf";
    rustToolchain = "rust";
  });
  android = check (android {
    inherit pkgs;
    timeZone = zone;
    androidSdk = "/fixture/android";
    ndkVersion = "1";
  });
  androidOverlay = let
    base = rec {
      env = {TZ = zone;};
      overrideAttrs = callback: base // callback base;
    };
  in
    check (android {
      inherit pkgs base;
      androidSdk = "/fixture/android";
      ndkVersion = "1";
    });
  projects = check (projects.mkDocsDevShell {
    inherit pkgs;
    timeZone = zone;
    plinthProject = "plinth";
  });
  genericArchive = checkArchive archive;
  portableArchive = checkArchive (portable.mkPortableBinaryRelease releaseArgs).archives.x86_64-linux;
  binaryArchive = checkArchive (binary.mkBinaryRelease releaseArgs).archives.x86_64-linux;
  artifactOverride =
    checkArchive
    (portable.mkPortableBinaryRelease (releaseArgs
      // {
        archiveTimezone = "UTC";
        artifacts.x86_64-linux = releaseArgs.artifacts.x86_64-linux // {archiveTimezone = zone;};
      })).archives.x86_64-linux;
}
