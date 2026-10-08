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
in
  pkgs.runCommand "harbor-artifact-acceptance-contract" {
    nativeBuildInputs = [pkgs.python3];
  } ''
    test -f ${checked}/.harbor/acceptance.json
    cmp ${artifact}/app/index.html ${checked}/app/index.html
    python3 ${../lib/artifact-acceptance.py} verify-local \
      --manifest ${checked}/.harbor/acceptance.json --root ${checked}
    HARBOR_ACCEPTANCE_MODULE=${../lib/artifact-acceptance.py} python3 ${./test-artifact-acceptance.py}
    touch "$out"
  ''
