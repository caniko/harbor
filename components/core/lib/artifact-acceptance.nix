{lib}: {
  # The command must exercise these immutable artifacts and write the normalized
  # report to HARBOR_ACCEPTANCE_REPORT. Required IDs bind the consumer contract;
  # zero tests, skipped tests, flaky retries and duplicate IDs cannot pass.
  mkCheckedArtifact = {
    pkgs,
    artifact,
    artifacts ? {},
    requiredTests,
    command,
    nativeBuildInputs ? [],
    name ? "${artifact.name}-accepted",
  }:
    assert lib.assertMsg (requiredTests != [] && lib.all (id: builtins.isString id && id != "") requiredTests)
    "harbor artifact acceptance: requiredTests must contain non-empty test IDs";
    assert lib.assertMsg (builtins.length requiredTests == builtins.length (lib.unique requiredTests))
    "harbor artifact acceptance: required test IDs must be unique";
    assert lib.assertMsg (command != "") "harbor artifact acceptance: command is required";
    assert lib.assertMsg (!(artifacts ? artifact)) "harbor artifact acceptance: artifact root is reserved"; let
      roots = pkgs.writeText "acceptance-roots.json" (builtins.toJSON (
        builtins.mapAttrs (_: value: toString value) (artifacts // {inherit artifact;})
      ));
      required = pkgs.writeText "required-tests.json" (builtins.toJSON requiredTests);
      verifier = import ../package.nix {inherit pkgs;};
    in
      pkgs.runCommand name {
        nativeBuildInputs = [verifier] ++ nativeBuildInputs;
        passthru = {inherit artifact requiredTests;};
      } ''
        export HARBOR_ARTIFACT=${lib.escapeShellArg (toString artifact)}
        export HARBOR_ARTIFACT_MANIFEST="$TMPDIR/artifact-identity.json"
        export HARBOR_ACCEPTANCE_REPORT="$TMPDIR/acceptance-report.json"
        harbor dev artifact snapshot --roots ${roots} --manifest "$HARBOR_ARTIFACT_MANIFEST"
        ${command}
        harbor dev artifact accept --manifest "$HARBOR_ARTIFACT_MANIFEST" \
          --report "$HARBOR_ACCEPTANCE_REPORT" --required ${required}
        mkdir -p "$out"
        cp -rL --no-preserve=mode ${artifact}/. "$out/"
        mkdir -p "$out/.harbor"
        cp "$HARBOR_ARTIFACT_MANIFEST" "$out/.harbor/acceptance.json"
        harbor prod artifact verify-local --manifest "$out/.harbor/acceptance.json" --root "$out"
      '';

  mkArtifactVerifier = {pkgs}:
    let package = import ../package.nix {inherit pkgs;}; in
      package // {meta = package.meta // {mainProgram = "harbor-verify-artifact";};};
}
