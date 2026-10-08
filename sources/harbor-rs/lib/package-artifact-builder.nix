{packageTests}: {
  pkgs ? null,
  kind,
  packageName,
  version,
  output,
  buildCommand ? null,
  inputs ? [],
  metadata ? {},
  unsupportedBuilderReason ? null,
}: let
  builderKind =
    if builtins.match ".*-builder" kind != null
    then kind
    else "${kind}-builder";
in
  packageTests.mkArtifactBuilder {
    kind = builderKind;
    inherit packageName version buildCommand inputs unsupportedBuilderReason;
    output = toString output;
    metadata =
      metadata
      // {
        rsHarborHelper = kind;
      };
  }
