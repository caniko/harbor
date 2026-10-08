# Reproducible Dioxus web and fullstack builders.
#
# Product flakes own source filtering, application dependencies, and deployment
# policy. rs-harbor owns the offline DX/Cargo/WASM mechanics, the compiler-cache
# policy, and the output contract shared by those products.
{packageTests}: let
  buildPlan = import ./dioxus-build-plan.nix;
  resolveWasmBindgenCli = import ./wasm-bindgen.nix;

  mkWasmOptCompat = {
    pkgs,
    wasmOptPackage,
    wasmOptArgs,
  }:
    pkgs.writeShellScriptBin "wasm-opt" ''
      set -uo pipefail
      if ${wasmOptPackage}/bin/wasm-opt ${pkgs.lib.escapeShellArgs wasmOptArgs} "$@"; then
        exit 0
      else
        status=$?
        if [ -n "''${RS_HARBOR_DIOXUS_WASM_OPT_FAILURE:-}" ]; then
          printf 'wasm-opt exited with status %s\n' "$status" > "$RS_HARBOR_DIOXUS_WASM_OPT_FAILURE"
        fi
        exit "$status"
      fi
    '';

  common = {
    src,
    pkgs,
    cargoLock,
    pname,
    version,
    rustToolchain,
    craneLib,
    cargoVendorDir,
    outputHashes,
    overrideVendorGitCheckout,
    package,
    binary,
    profile,
    debugSymbols,
    noDefaultFeatures,
    sharedFeatures,
    webFeatures,
    serverFeatures,
    wasmTarget,
    wasmSplit,
    cargoArgs,
    wasmBindgenCli,
    allowWasmBindgenMismatch,
    dioxusCli,
    wasmOptPackage,
    wasmOptArgs,
    nativeBuildInputs,
    fullstack,
    buildCache ? (craneLib.rsHarborBuildCachePolicy or null),
  }: let
    lib = pkgs.lib;
    lockPath =
      if builtins.isPath cargoLock
      then cargoLock
      else cargoLock.lockFile or cargoLock;
    vendorArgs =
      {
        inherit src;
        cargoLock = lockPath;
        inherit outputHashes;
      }
      // lib.optionalAttrs (overrideVendorGitCheckout != null) {
        inherit overrideVendorGitCheckout;
      };
    effectiveCargoVendorDir =
      if cargoVendorDir != null
      then cargoVendorDir
      else craneLib.vendorCargoDeps vendorArgs;
    resolvedWasmBindgen = resolveWasmBindgenCli {
      inherit pkgs cargoLock lib;
      wasmBindgenCli = wasmBindgenCli;
      allowMismatch = allowWasmBindgenMismatch;
    };
    plan = buildPlan {
      inherit
        lib
        package
        binary
        profile
        debugSymbols
        noDefaultFeatures
        sharedFeatures
        webFeatures
        serverFeatures
        wasmTarget
        wasmSplit
        cargoArgs
        fullstack
        ;
    };
    wasmOptCompat =
      if wasmOptPackage == null
      then null
      else mkWasmOptCompat {inherit pkgs wasmOptPackage wasmOptArgs;};
    nativeInputs =
      [rustToolchain dioxusCli pkgs.pkg-config resolvedWasmBindgen.package]
      ++ lib.optional pkgs.stdenv.isLinux pkgs.clang
      ++ lib.optional pkgs.stdenv.isLinux pkgs.mold
      ++ lib.optional (wasmOptCompat != null) wasmOptCompat
      ++ [pkgs.esbuild]
      ++ lib.optional (buildCache != null) buildCache.wrapper
      ++ nativeBuildInputs;
  in {
    cacheEnv =
      if buildCache == null
      then {}
      else buildCache.dioxusEnv;
    inherit plan resolvedWasmBindgen effectiveCargoVendorDir nativeInputs;
    inherit lib lockPath;
  };

  mkWeb = args @ {
    src,
    pkgs,
    cargoLock,
    pname,
    version ? "0.1.0",
    rustToolchain,
    craneLib,
    cargoVendorDir ? null,
    outputHashes ? {},
    overrideVendorGitCheckout ? null,
    package ? null,
    binary ? null,
    profile ? "release",
    debugSymbols ? profile != "release",
    noDefaultFeatures ? true,
    sharedFeatures ? [],
    webFeatures ? ["web"],
    serverFeatures ? ["server"],
    wasmTarget ? "wasm32-unknown-unknown",
    features ? null,
    wasmSplit ? false,
    cargoArgs ? {
      web = [];
      server = [];
    },
    wasmBindgenCli ? null,
    allowWasmBindgenMismatch ? false,
    dioxusCli ? pkgs.dioxus-cli,
    # Dioxus always runs wasm-opt during bundling. Keep binaryen in the
    # hermetic builder by default; callers can pass null only when their
    # Dioxus CLI is wrapped with an equivalent optimizer.
    wasmOptPackage ? pkgs.binaryen,
    wasmOptArgs ? [],
    installSubdir ? "share/${pname}",
    nativeBuildInputs ? [],
    fullstack ? false,
    buildCache ? (craneLib.rsHarborBuildCachePolicy or null),
    ...
  }: let
    effectiveWebFeatures =
      if features == null
      then webFeatures
      else features;
    extraArgs = builtins.removeAttrs args [
      "src"
      "pkgs"
      "cargoLock"
      "pname"
      "version"
      "rustToolchain"
      "craneLib"
      "cargoVendorDir"
      "outputHashes"
      "overrideVendorGitCheckout"
      "package"
      "binary"
      "profile"
      "debugSymbols"
      "noDefaultFeatures"
      "sharedFeatures"
      "webFeatures"
      "serverFeatures"
      "wasmTarget"
      "features"
      "wasmSplit"
      "cargoArgs"
      "wasmBindgenCli"
      "allowWasmBindgenMismatch"
      "dioxusCli"
      "wasmOptPackage"
      "wasmOptArgs"
      "installSubdir"
      "nativeBuildInputs"
      "fullstack"
      "buildCache"
    ];
    c = common {
      inherit
        src
        pkgs
        cargoLock
        pname
        version
        rustToolchain
        craneLib
        cargoVendorDir
        outputHashes
        overrideVendorGitCheckout
        package
        binary
        profile
        debugSymbols
        noDefaultFeatures
        sharedFeatures
        serverFeatures
        wasmTarget
        wasmSplit
        cargoArgs
        wasmBindgenCli
        allowWasmBindgenMismatch
        dioxusCli
        wasmOptPackage
        wasmOptArgs
        nativeBuildInputs
        fullstack
        buildCache
        ;
      webFeatures = effectiveWebFeatures;
    };
    packageDrv = pkgs.stdenvNoCC.mkDerivation ({
        inherit src pname version;
        nativeBuildInputs = c.nativeInputs;
        env = c.cacheEnv;
        dontConfigure = true;
        buildPhase = ''
          runHook preBuild
          export HOME="$TMPDIR/home"
          export CARGO_HOME="$TMPDIR/cargo-home"
          export CARGO_TARGET_DIR="$TMPDIR/cargo-target"
          export CARGO_NET_OFFLINE=true
          export RS_HARBOR_DIOXUS_WASM_OPT_FAILURE="$TMPDIR/wasm-opt-failed"
          mkdir -p "$HOME" "$CARGO_HOME" "$CARGO_TARGET_DIR" .cargo
          if [ -f .cargo/config.toml ]; then
            cp .cargo/config.toml "$TMPDIR/project-cargo-config.toml"
          else
            : > "$TMPDIR/project-cargo-config.toml"
          fi
          cp "${c.effectiveCargoVendorDir}/config.toml" .cargo/config.toml
          chmod u+w .cargo/config.toml
          cat "$TMPDIR/project-cargo-config.toml" >> .cargo/config.toml
          dx ${pkgs.lib.escapeShellArgs c.plan.dxCommon} --out-dir "$TMPDIR/dioxus-out" ${pkgs.lib.escapeShellArgs c.plan.dxSuffix}
          if [ -e "$RS_HARBOR_DIOXUS_WASM_OPT_FAILURE" ]; then
            cat "$RS_HARBOR_DIOXUS_WASM_OPT_FAILURE" >&2
            echo "rs-harbor: Dioxus ignored a failed wasm-opt invocation" >&2
            exit 1
          fi
          test -s "$TMPDIR/dioxus-out/public/index.html"
          runHook postBuild
        '';
        installPhase = ''
          runHook preInstall
          mkdir -p "$out/${installSubdir}"
          cp -R "$TMPDIR/dioxus-out/public"/. "$out/${installSubdir}/"
          test -s "$out/${installSubdir}/index.html"
          runHook postInstall
        '';
      }
      // extraArgs);
  in
    packageDrv
    // {
      passthru.dioxus =
        c.plan.metadata
        // {
          schemaVersion = 1;
          kind = "web";
          publicDir = installSubdir;
          wasmBindgenVersion = c.resolvedWasmBindgen.version;
          dioxusCliVersion = dioxusCli.version or null;
        };
      artifactBuilder = packageTests.mkArtifactBuilder {
        kind = "trunk-builder";
        packageName = pname;
        inherit version;
        output = toString packageDrv;
        buildCommand = "nix build .#${pname}";
        inputs = [(toString src)];
        metadata = {
          helper = "mkDioxusWebPackage";
          inherit (c.plan.metadata) platform features wasmSplit debugSymbols;
          wasmBindgenVersion = c.resolvedWasmBindgen.version;
        };
      };
    };

  mkFullstack = args @ {
    src,
    pkgs,
    cargoLock,
    pname,
    version ? "0.1.0",
    rustToolchain,
    craneLib,
    cargoVendorDir ? null,
    outputHashes ? {},
    overrideVendorGitCheckout ? null,
    package ? null,
    binary ? null,
    serverInstallName ? pname,
    serverBinary ? "server",
    profile ? "release",
    debugSymbols ? profile != "release",
    noDefaultFeatures ? true,
    sharedFeatures ? [],
    webFeatures ? ["web"],
    serverFeatures ? ["server"],
    wasmTarget ? "wasm32-unknown-unknown",
    wasmSplit ? false,
    cargoArgs ? {
      web = [];
      server = [];
    },
    wasmBindgenCli ? null,
    allowWasmBindgenMismatch ? false,
    dioxusCli ? pkgs.dioxus-cli,
    wasmOptPackage ? pkgs.binaryen,
    wasmOptArgs ? [],
    publicSubdir ? "share/${pname}/public",
    wrapServer ? true,
    nativeBuildInputs ? [],
    buildCache ? (craneLib.rsHarborBuildCachePolicy or null),
    ...
  }: let
    extraArgs = builtins.removeAttrs args [
      "src"
      "pkgs"
      "cargoLock"
      "pname"
      "version"
      "rustToolchain"
      "craneLib"
      "cargoVendorDir"
      "outputHashes"
      "overrideVendorGitCheckout"
      "package"
      "binary"
      "serverInstallName"
      "serverBinary"
      "profile"
      "debugSymbols"
      "noDefaultFeatures"
      "sharedFeatures"
      "webFeatures"
      "serverFeatures"
      "wasmTarget"
      "wasmSplit"
      "cargoArgs"
      "wasmBindgenCli"
      "allowWasmBindgenMismatch"
      "dioxusCli"
      "wasmOptPackage"
      "wasmOptArgs"
      "publicSubdir"
      "wrapServer"
      "nativeBuildInputs"
      "buildCache"
    ];
    c = common {
      inherit
        src
        pkgs
        cargoLock
        pname
        version
        rustToolchain
        craneLib
        cargoVendorDir
        outputHashes
        overrideVendorGitCheckout
        package
        binary
        profile
        debugSymbols
        noDefaultFeatures
        sharedFeatures
        webFeatures
        serverFeatures
        wasmTarget
        wasmSplit
        cargoArgs
        wasmBindgenCli
        allowWasmBindgenMismatch
        dioxusCli
        wasmOptPackage
        wasmOptArgs
        nativeBuildInputs
        buildCache
        ;
      fullstack = true;
    };
    packageDrv = pkgs.stdenvNoCC.mkDerivation ({
        inherit src pname version;
        nativeBuildInputs = c.nativeInputs;
        env = c.cacheEnv;
        dontConfigure = true;
        buildPhase = ''
          runHook preBuild
          export HOME="$TMPDIR/home"
          export CARGO_HOME="$TMPDIR/cargo-home"
          export CARGO_TARGET_DIR="$TMPDIR/cargo-target"
          export CARGO_NET_OFFLINE=true
          export RS_HARBOR_DIOXUS_WASM_OPT_FAILURE="$TMPDIR/wasm-opt-failed"
          mkdir -p "$HOME" "$CARGO_HOME" "$CARGO_TARGET_DIR" .cargo
          if [ -f .cargo/config.toml ]; then
            cp .cargo/config.toml "$TMPDIR/project-cargo-config.toml"
          else
            : > "$TMPDIR/project-cargo-config.toml"
          fi
          cp "${c.effectiveCargoVendorDir}/config.toml" .cargo/config.toml
          chmod u+w .cargo/config.toml
          cat "$TMPDIR/project-cargo-config.toml" >> .cargo/config.toml
          dx ${pkgs.lib.escapeShellArgs c.plan.dxCommon} --out-dir "$TMPDIR/dioxus-out" ${pkgs.lib.escapeShellArgs c.plan.dxSuffix}
          if [ -e "$RS_HARBOR_DIOXUS_WASM_OPT_FAILURE" ]; then
            cat "$RS_HARBOR_DIOXUS_WASM_OPT_FAILURE" >&2
            echo "rs-harbor: Dioxus ignored a failed wasm-opt invocation" >&2
            exit 1
          fi
          test -s "$TMPDIR/dioxus-out/public/index.html"
          test -x "$TMPDIR/dioxus-out/${serverBinary}"
          runHook postBuild
        '';
        installPhase = ''
          runHook preInstall
          mkdir -p "$out/bin" "$out/${publicSubdir}"
          cp "$TMPDIR/dioxus-out/${serverBinary}" "$out/bin/${serverInstallName}-unwrapped"
          chmod +x "$out/bin/${serverInstallName}-unwrapped"
          cp -R "$TMPDIR/dioxus-out/public"/. "$out/${publicSubdir}/"
          test -s "$out/${publicSubdir}/index.html"
          ${pkgs.lib.optionalString wrapServer ''
            cat > "$out/bin/${serverInstallName}" <<EOF
            #!/bin/sh
            export DIOXUS_PUBLIC_PATH="\''${DIOXUS_PUBLIC_PATH:-$out/${publicSubdir}}"
            exec "$out/bin/${serverInstallName}-unwrapped" "\$@"
            EOF
            chmod +x "$out/bin/${serverInstallName}"
          ''}
          ${pkgs.lib.optionalString (!wrapServer) ''
            mv "$out/bin/${serverInstallName}-unwrapped" "$out/bin/${serverInstallName}"
          ''}
          runHook postInstall
        '';
      }
      // extraArgs);
  in
    packageDrv
    // {
      passthru.dioxus =
        c.plan.metadata
        // {
          schemaVersion = 1;
          kind = "fullstack";
          publicDir = publicSubdir;
          serverProgram = "bin/${serverInstallName}";
          inherit serverBinary;
          wasmBindgenVersion = c.resolvedWasmBindgen.version;
          dioxusCliVersion = dioxusCli.version or null;
        };
      artifactBuilder = packageTests.mkArtifactBuilder {
        kind = "trunk-builder";
        packageName = pname;
        inherit version;
        output = toString packageDrv;
        buildCommand = "nix build .#${pname}";
        inputs = [(toString src)];
        metadata = {
          helper = "mkDioxusFullstackPackage";
          inherit (c.plan.metadata) platform features wasmSplit debugSymbols;
          wasmBindgenVersion = c.resolvedWasmBindgen.version;
          serverProgram = "bin/${serverInstallName}";
        };
      };
    };
in {
  inherit mkFullstack mkWeb;
  mkDioxusWebPackage = mkWeb;
  mkDioxusFullstackPackage = mkFullstack;

  # Compatibility name retained for one downstream migration cycle. Keep the
  # historical artifact metadata while routing the implementation through the
  # explicit web builder.
  mkDioxusPackage = args: let
    drv = mkWeb args;
  in
    drv
    // {
      artifactBuilder =
        drv.artifactBuilder
        // {
          metadata = drv.artifactBuilder.metadata // {helper = "mkDioxusPackage";};
        };
    };
}
