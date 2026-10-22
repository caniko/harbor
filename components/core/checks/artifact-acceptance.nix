{
  pkgs,
  packageTests,
}: let
  artifact = pkgs.runCommand "acceptance-fixture" {} ''
    mkdir -p "$out/app"
    echo fixture > "$out/app/index.html"
  '';
  checked = packageTests.mkCheckedArtifact {
    inherit pkgs artifact;
    requiredTests = ["review-progress"];
    command = ''
      echo '{"tests":[{"id":"review-progress","status":"passed"}]}' > "$HARBOR_ACCEPTANCE_REPORT"
    '';
  };
  verifier = packageTests.mkArtifactVerifier {inherit pkgs;};
in
  pkgs.runCommand "harbor-artifact-acceptance-contract" {
    nativeBuildInputs = [verifier];
  } ''
    test -f ${checked}/.harbor/acceptance.json
    cmp ${artifact}/app/index.html ${checked}/app/index.html
    harbor-verify-artifact verify-local \
      --manifest ${checked}/.harbor/acceptance.json --root ${checked}
    # Native Rust package tests include byte substitution, missing/extra assets,
    # required test IDs, HTTP drift/redirects, and compatibility CLI contracts.
    touch "$out"
  ''
