# Builds the Bevy game crate using crane (from harbor-rs).
#
# Uses a two-phase build: cargoArtifacts caches dependency compilation,
# then buildPackage compiles the actual game crate.
{
  craneLib,
  bevyDeps,
  src,
}: let
  commonArgs = {
    inherit src;
    strictDeps = true;

    buildInputs = bevyDeps.buildInputs;
    nativeBuildInputs = bevyDeps.nativeBuildInputs;
  };

  cargoArtifacts = craneLib.buildDepsOnly commonArgs;
in {
  inherit cargoArtifacts commonArgs;

  default = craneLib.buildPackage (commonArgs
    // {
      inherit cargoArtifacts;
    });

  clippy = craneLib.cargoClippy (commonArgs
    // {
      inherit cargoArtifacts;
      cargoClippyExtraArgs = "--all-targets -- --deny warnings";
    });

  fmt = craneLib.cargoFmt {
    inherit src;
  };
}
