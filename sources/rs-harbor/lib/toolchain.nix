# mkToolchain :: { pkgs, toolchainProfile?, toolchainFile?, channel?, date?, extensions?, withRustAnalyzer?, crossTargets?, cache? }
#             -> { rustToolchain, craneLib, rawCraneLib, buildCache, cargoConfig, crossTargets }
#
# Build a Rust toolchain + craneLib for a given pkgs set.
{
  crane,
  mkCargoConfig,
  mkBuildCachePolicy,
}: {
  pkgs,
  # Optional rs-harbor-owned pinned profile. Null preserves the historical
  # channel/date behavior for consumers that are not opting into fleet pins.
  toolchainProfile ? null,
  # When set, the standard rust-toolchain.toml is authoritative for the
  # compiler channel, components, and targets. The legacy channel/date mode
  # remains the default when this is null.
  toolchainFile ? null,
  channel ? null,
  # Preserve the historical channel default for downstream stable consumers.
  # Projects that need the fleet-pinned nightly read `rust-toolchain.toml` and
  # pass its date explicitly; a stable channel has no nightly date attribute.
  date ? null,
  extensions ? null,
  withRustAnalyzer ? true,
  crossTargets ? null,
  # Compiler caching requires host transport and is therefore explicit.
  cache ? {},
}:
assert pkgs.lib.assertMsg (pkgs ? rust-bin)
"rs-harbor: mkToolchain requires pkgs with rust-overlay applied (pkgs.rust-bin must exist)";
assert pkgs.lib.assertMsg (toolchainProfile == null || builtins.elem toolchainProfile ["nightly" "stable"])
"rs-harbor: mkToolchain 'toolchainProfile' must be \"nightly\" or \"stable\", got \"${toString toolchainProfile}\"";
assert pkgs.lib.assertMsg (toolchainProfile == null || (toolchainFile == null && channel == null && date == null))
"rs-harbor: mkToolchain 'toolchainProfile' cannot be combined with 'toolchainFile', 'channel', or 'date'";
assert pkgs.lib.assertMsg (toolchainFile == null || (channel == null && date == null))
"rs-harbor: mkToolchain 'toolchainFile' cannot be combined with 'channel' or 'date'";
assert pkgs.lib.assertMsg (channel == null || builtins.elem channel ["nightly" "stable"])
"rs-harbor: mkToolchain 'channel' must be \"nightly\" or \"stable\", got \"${toString channel}\"";
assert pkgs.lib.assertMsg (date == null || date == "latest" || builtins.match "[0-9]{4}-[0-9]{2}-[0-9]{2}" date != null)
"rs-harbor: mkToolchain 'date' must be \"latest\" or a YYYY-MM-DD string, got \"${toString date}\""; let
  profileToolchainFiles = {
    nightly = ../rust-toolchain.toml;
    stable = ../rust-toolchain-stable.toml;
  };
  resolvedToolchainFile =
    if toolchainProfile != null
    then profileToolchainFiles.${toolchainProfile}
    else toolchainFile;
  legacyExtensions =
    if extensions == null
    then ["rust-src" "rustfmt" "rustc-codegen-cranelift-preview" "llvm-tools-preview"]
    else extensions;
  legacyCrossTargets =
    if crossTargets == null
    then [
      "x86_64-unknown-linux-gnu"
      "aarch64-unknown-linux-gnu"
      "x86_64-pc-windows-gnu"
      "x86_64-apple-darwin"
      "aarch64-apple-darwin"
    ]
    else crossTargets;
  toolchainManifest =
    if resolvedToolchainFile == null
    then {}
    else builtins.fromTOML (builtins.readFile resolvedToolchainFile);
  manifestToolchain = toolchainManifest.toolchain or {};
  manifestComponents = manifestToolchain.components or [];
  manifestTargets = manifestToolchain.targets or [];
  fileExtensions =
    manifestComponents
    ++ (
      if extensions == null
      then []
      else extensions
    )
    ++ (
      if
        withRustAnalyzer
        && !(builtins.elem "rust-analyzer" (manifestComponents
          ++ (
            if extensions == null
            then []
            else extensions
          )))
      then ["rust-analyzer"]
      else []
    );
  fileTargets =
    manifestTargets
    ++ (
      if crossTargets == null
      then []
      else crossTargets
    );
  resolvedCrossTargets =
    if resolvedToolchainFile != null
    then fileTargets
    else legacyCrossTargets;
  legacyExtensions' =
    if withRustAnalyzer
    then
      if builtins.elem "rust-analyzer" legacyExtensions
      then legacyExtensions
      else legacyExtensions ++ ["rust-analyzer"]
    else legacyExtensions;
  legacyChannel =
    if channel == null
    then "nightly"
    else channel;
  legacyDate =
    if date == null
    then "latest"
    else date;
  manifestChannel = manifestToolchain.channel or "";
  cargoChannel =
    if resolvedToolchainFile == null
    then legacyChannel
    else if builtins.match "nightly.*" manifestChannel != null
    then "nightly"
    else "stable";
  resolvedToolchainManifest =
    if resolvedToolchainFile != null
    then manifestToolchain
    else {
      channel =
        if legacyChannel == "nightly" && legacyDate != "latest"
        then "nightly-${legacyDate}"
        else legacyChannel;
      components = legacyExtensions';
      targets = legacyCrossTargets;
    };
  channelSet =
    if legacyChannel == "nightly"
    then pkgs.rust-bin.nightly
    else pkgs.rust-bin.stable;
  dateSet =
    if legacyDate == "latest"
    then channelSet.latest
    else channelSet.${legacyDate};
  rustToolchain =
    if resolvedToolchainFile != null
    then
      assert pkgs.lib.assertMsg (manifestToolchain ? channel)
      "rs-harbor: mkToolchain 'toolchainFile' must contain [toolchain].channel";
        (pkgs.rust-bin.fromRustupToolchainFile resolvedToolchainFile).override {
          extensions = fileExtensions;
          targets = fileTargets;
        }
    else
      dateSet.default.override {
        extensions = legacyExtensions';
        targets = legacyCrossTargets;
      };
  toolchainArgs =
    (
      if resolvedToolchainFile != null
      then {toolchainFile = resolvedToolchainFile;}
      else {
        channel = legacyChannel;
        date = legacyDate;
      }
    )
    // pkgs.lib.optionalAttrs (extensions != null) {inherit extensions;}
    // pkgs.lib.optionalAttrs (!withRustAnalyzer) {withRustAnalyzer = false;};
  cargoConfig = mkCargoConfig {
    inherit pkgs;
    channel = cargoChannel;
    crossTargets = resolvedCrossTargets;
  };
  upstreamCraneLib = (crane.mkLib pkgs).overrideToolchain (_p: rustToolchain);

  patchCratesIoPathPatches = args: let
    # `cleanSourceWith` returns a source accessor whose filtered `outPath` may
    # not be realised while package arguments are still being evaluated. Read
    # Cargo.toml from its durable original source instead. Clean-source
    # accessors can also carry a stale wrapper path in their string context,
    # so classify that context and reconstruct only a proven static store path.
    # This keeps evaluation independent of prior build and store state.
    cargoTomlSource =
      if args ? src && (args.src._isLibCleanSourceWith or false)
      then args.src.origSrc
      else args.src or null;
    cargoTomlSourceStringResult =
      if cargoTomlSource != null
      then builtins.tryEval (toString cargoTomlSource)
      else {
        success = false;
        value = null;
      };
    cargoTomlSourceString =
      if cargoTomlSourceStringResult.success
      then cargoTomlSourceStringResult.value
      else null;
    cargoTomlSourceContext =
      if cargoTomlSourceString != null
      then builtins.getContext cargoTomlSourceString
      else {};
    cargoTomlSourceContextValues = builtins.attrValues cargoTomlSourceContext;
    cargoTomlSourceHasOnlyPathContext =
      cargoTomlSourceContextValues
      != []
      && builtins.all
      (context: builtins.attrNames context == ["path"] && context.path)
      cargoTomlSourceContextValues;
    cargoTomlSourcePlain =
      if cargoTomlSourceString != null
      then builtins.unsafeDiscardStringContext cargoTomlSourceString
      else null;
    cargoTomlSourceIsStorePath =
      cargoTomlSourcePlain
      != null
      && pkgs.lib.hasPrefix (builtins.storeDir + "/") cargoTomlSourcePlain;
    cargoTomlSourceIsSafe =
      cargoTomlSourceString
      != null
      && (
        builtins.isPath cargoTomlSource
        || (cargoTomlSourceHasOnlyPathContext && cargoTomlSourceIsStorePath)
      );
    cargoTomlPath =
      if builtins.isPath cargoTomlSource
      then cargoTomlSource + "/Cargo.toml"
      else if cargoTomlSourceHasOnlyPathContext && cargoTomlSourceIsStorePath
      then (/. + cargoTomlSourcePlain) + "/Cargo.toml"
      else null;
    cargoTomlRead =
      if args ? rsHarborCargoTomlContents
      then
        if builtins.isString args.rsHarborCargoTomlContents
        then {
          success = true;
          value = args.rsHarborCargoTomlContents;
        }
        else throw "rs-harbor: rsHarborCargoTomlContents must be a string"
      else if cargoTomlSource != null && !cargoTomlSourceIsSafe
      then
        throw ''
          rs-harbor: cannot safely inspect Cargo.toml from this `src` during evaluation.

          Generated derivation outputs and untracked plain-string paths would make path-patch validation depend on prior store or host state. Pass `rsHarborCargoTomlContents = builtins.readFile ./Cargo.toml` so validation has explicit source data, or pass a direct Nix path / lib.cleanSourceWith source as `src`.
        ''
      else if cargoTomlPath != null
      then builtins.tryEval (builtins.readFile cargoTomlPath)
      else {
        success = false;
        value = null;
      };
    cargoToml =
      if cargoTomlRead.success
      then builtins.fromTOML (builtins.unsafeDiscardStringContext cargoTomlRead.value)
      else {};
    cratesIoPatches = cargoToml.patch."crates-io" or {};
    patchNames = builtins.attrNames cratesIoPatches;
    pathPatchNames = builtins.filter (name: (cratesIoPatches.${name} ? path)) patchNames;
  in
    builtins.map (name: {
      inherit name;
      path = cratesIoPatches.${name}.path;
    })
    pathPatchNames;

  formatPathPatches = pathPatches:
    pkgs.lib.concatStringsSep ", " (
      builtins.map (patch: "${patch.name} -> ${patch.path}") pathPatches
    );

  stripRsHarborCraneArgs = args:
    builtins.removeAttrs args [
      "rsHarborAllowPathPatchBuildDepsOnly"
      "rsHarborCargoTomlContents"
      "rsHarborCacheReuseKey"
    ];

  pathPatchBuildDepsOnlyError = pathPatches: ''
    rs-harbor: refusing craneLib.buildDepsOnly for a Cargo workspace with [patch.crates-io] path patches: ${formatPathPatches pathPatches}

    Crane's dependency-only build replaces path crates with dummy sources. That is unsafe when a patched crate is used by registry dependencies, because those dependencies can compile against a dummy API and fail with misleading missing-symbol errors.

    Use craneLib.buildPackage (args // { cargoArtifacts = null; }) to build with real sources, remove the path patch, or vendor the dependency in a way that does not rely on [patch.crates-io]. If this project is known to tolerate dummy path patches, pass rsHarborAllowPathPatchBuildDepsOnly = true.
  '';

  cacheEnabled = cache.enable or false;
  buildCache =
    if cacheEnabled
    then
      mkBuildCachePolicy {
        inherit pkgs;
        compiler = rustToolchain.version;
        toolchainManifest = resolvedToolchainManifest;
        buildPackageSet = pkgs.buildPackages;
        sccachePackage = cache.sccachePackage or null;
        cacheRoot = cache.cacheRoot or null;
        namespaceScope = cache.namespaceScope or "canix-rust";
        namespaceGeneration = cache.namespaceGeneration or 5;
        connectTimeout = cache.connectTimeout or "2";
        executionModel = cache.executionModel or "sandbox-local";
        redisSocketPath = cache.redisSocketPath or "/run/redis-sccache/redis.sock";
      }
    else null;

  # Keep rs-harbor's path-patch safety checks at the crane scope boundary.
  # The cache wrapper is applied in the same scope so every crane helper that
  # constructs a Cargo derivation inherits it automatically.
  craneLib =
    (upstreamCraneLib.overrideScope (_final: prev: {
      mkCargoDerivation =
        if buildCache == null
        then prev.mkCargoDerivation
        else args: buildCache.withRustCache {package = prev.mkCargoDerivation args;};

      buildDepsOnly = args: let
        allow = args.rsHarborAllowPathPatchBuildDepsOnly or false;
        pathPatches =
          if allow
          then []
          else patchCratesIoPathPatches args;
        raw =
          if pathPatches != []
          then throw (pathPatchBuildDepsOnlyError pathPatches)
          else prev.buildDepsOnly (stripRsHarborCraneArgs args);
      in
        if buildCache == null
        then raw
        else
          buildCache.withRustCache {
            package = raw;
            telemetry = {
              workloadKind = "dependency-artifacts";
              reuseKey = args.rsHarborCacheReuseKey or "";
            };
          };

      buildPackage = args: let
        pathPatches =
          if args ? cargoArtifacts
          then []
          else patchCratesIoPathPatches args;
        args' = stripRsHarborCraneArgs args;
        raw = prev.buildPackage (
          if pathPatches != [] && !(args ? cargoArtifacts)
          then args' // {cargoArtifacts = null;}
          else args'
        );
      in
        if buildCache == null
        then raw
        else
          buildCache.withRustCache {
            package = raw;
            telemetry = {
              workloadKind = "package";
              reuseKey = args.rsHarborCacheReuseKey or "";
            };
          };
    }))
    // {
      # Dioxus and cross helpers use these markers to inherit the same policy
      # without requiring every consumer to thread it through manually.
      rsHarborBuildCachePolicy = buildCache;
      rsHarborCargoConfig = cargoConfig;
      rsHarborToolchainArgs = toolchainArgs;
      rsHarborToolchainFile = resolvedToolchainFile;
      rsHarborToolchainProfile = toolchainProfile;
    };

  rawCraneLib = upstreamCraneLib.overrideScope (_final: prev: {
    buildDepsOnly = args: let
      allow = args.rsHarborAllowPathPatchBuildDepsOnly or false;
      pathPatches =
        if allow
        then []
        else patchCratesIoPathPatches args;
    in
      if pathPatches != []
      then throw (pathPatchBuildDepsOnlyError pathPatches)
      else prev.buildDepsOnly (stripRsHarborCraneArgs args);

    buildPackage = args: let
      pathPatches =
        if args ? cargoArtifacts
        then []
        else patchCratesIoPathPatches args;
      args' = stripRsHarborCraneArgs args;
    in
      prev.buildPackage (
        if pathPatches != [] && !(args ? cargoArtifacts)
        then args' // {cargoArtifacts = null;}
        else args'
      );
  });
in {
  inherit rustToolchain craneLib rawCraneLib buildCache cargoConfig toolchainArgs;
  inherit toolchainProfile resolvedToolchainFile resolvedToolchainManifest;
  crossTargets = resolvedCrossTargets;
}
