{
  pkgs,
  harbor,
}: let
  app = pkgs.runCommand "site-fixture-app" {} ''
    mkdir -p "$out"
    echo app > "$out/index.html"
  '';
  landing = pkgs.runCommand "site-fixture-landing" {} ''
    mkdir -p "$out"
    echo landing > "$out/index.html"
  '';
  args = {
    inherit pkgs;
    projectSiteLib.mkProjectSite = _: landing;
    pname = "site-fixture";
    domain = "fixture.invalid";
    configPath = ../templates/default/docs/book.toml;
    docs = landing;
    appSource = app;
  };
  unchecked = builtins.tryEval (harbor.mkSite args).drvPath;
  checked = harbor.mkSite (args
    // {
      acceptance = {
        requiredTests = ["assembled-app"];
        command = ''
          cmp "$HARBOR_ARTIFACT/app/index.html" ${app}/index.html
          cmp "$HARBOR_ARTIFACT/index.html" ${landing}/index.html
          echo '{"tests":[{"id":"assembled-app","status":"passed"}]}' > "$HARBOR_ACCEPTANCE_REPORT"
        '';
      };
    });
in
  assert !unchecked.success;
    pkgs.runCommand "harbor-projects-site-acceptance" {} ''
      test -s ${checked}/.harbor/acceptance.json
      cmp ${checked}/app/index.html ${app}/index.html
      touch "$out"
    ''
