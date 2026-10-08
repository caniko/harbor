# Pure Dioxus feature/platform argument normalization shared by web and
# fullstack builders.
{
  lib,
  package ? null,
  binary ? package,
  profile ? "release",
  debugSymbols ? profile != "release",
  noDefaultFeatures ? true,
  sharedFeatures ? [],
  webFeatures ? ["web"],
  serverFeatures ? ["server"],
  wasmTarget ? "wasm32-unknown-unknown",
  wasmSplit ? false,
  fullstack ? false,
  cargoArgs ? {
    web = [];
    server = [];
  },
}: let
  webFeatureSet = lib.unique (sharedFeatures ++ webFeatures);
  serverFeatureSet = lib.unique (sharedFeatures ++ serverFeatures);
  splitEnabled = wasmSplit;
  featureArgs = features:
    lib.optional noDefaultFeatures "--no-default-features"
    ++ lib.optional (features != []) "--features"
    ++ lib.optional (features != []) (lib.concatStringsSep " " features);
  targetCargoArgs = target: let
    args =
      if builtins.isAttrs cargoArgs
      then cargoArgs.${target} or []
      else cargoArgs;
  in
    lib.optional (args != []) "--cargo-args"
    ++ lib.optional (args != []) (lib.concatStringsSep " " args);
  profileArgs =
    if profile == "release"
    then ["--release"]
    else ["--profile" profile];
  dxCommon =
    ["--frozen" "bundle" "--platform" "web"]
    ++ lib.optional fullstack "--fullstack"
    ++ lib.optional (package != null) "--package"
    ++ lib.optional (package != null) package
    ++ lib.optional (binary != null) "--bin"
    ++ lib.optional (binary != null) binary
    ++ profileArgs
    ++ [
      "--debug-symbols=${
        if debugSymbols
        then "true"
        else "false"
      }"
    ]
    ++ lib.optional splitEnabled "--wasm-split";
  targetArgs = target:
    featureArgs (
      if target == "web"
      then webFeatureSet
      else serverFeatureSet
    )
    ++ lib.optional (target == "web") "--target"
    ++ lib.optional (target == "web") wasmTarget
    ++ targetCargoArgs target;
  dxSuffix =
    if fullstack
    then ["@client"] ++ targetArgs "web" ++ ["@server" "--server"] ++ targetArgs "server"
    else targetArgs "web";
  dxArgs =
    dxCommon ++ dxSuffix;
in
  assert lib.assertMsg (profile != "") "rs-harbor.mkDioxusBuildPlan: profile must not be empty"; {
    inherit dxArgs dxCommon dxSuffix;
    webCargoArgs = targetArgs "web";
    serverCargoArgs = targetArgs "server";
    metadata = {
      platform = "web";
      features =
        if fullstack
        then sharedFeatures
        else webFeatureSet;
      inherit package binary profile debugSymbols noDefaultFeatures wasmTarget wasmSplit fullstack;
      featureSets = {
        shared = sharedFeatures;
        web = webFeatures;
        server = serverFeatures;
      };
    };
  }
