{packageTests}: {
  pkgs,
  kind,
  name ? null,
  packageName ? name,
  version ? artifactBuilder.version,
  artifact ? artifactBuilder.output,
  artifactName ? builtins.baseNameOf (toString artifact),
  artifactBuilder ? null,
  installCommand ? "install ${packageName}",
  verify ? [],
  metadata ? {},
  unsupportedRunnerReason ? null,
}: let
  effectivePackageName =
    if packageName != null
    then packageName
    else artifactBuilder.packageName;
  effectiveVersion =
    if version != null
    then version
    else artifactBuilder.version;
  effectiveArtifact =
    if artifact != null
    then artifact
    else artifactBuilder.output;
  effectiveArtifactName =
    if artifactName != null
    then artifactName
    else builtins.baseNameOf (toString effectiveArtifact);
  plan = packageTests.mkPlan {
    inherit kind metadata;
    packageName = effectivePackageName;
    version = effectiveVersion;
    artifacts = [
      {
        name = effectiveArtifactName;
        path = toString effectiveArtifact;
      }
    ];
    install.command = installCommand;
    verify = map (command: {inherit command;}) verify;
    builder = artifactBuilder;
  };
in
  plan
  // {
    runner =
      if unsupportedRunnerReason == null
      then null
      else null;
    inherit unsupportedRunnerReason;
  }
