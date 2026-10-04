{
  pkgs,
  harborRsCli,
  mkDevSandbox,
}: let
  sandbox = mkDevSandbox {
    inherit pkgs harborRsCli;
    name = "fixture-sandbox";
    devShell = pkgs.mkShell {
      passthru.devShellSpec = {packages = [pkgs.hello];};
    };
    inputs = ["src" "Cargo.lock"];
    command = ["hello"];
    desktop = "headless";
  };
in
  pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
    dev-sandbox-contract = assert sandbox.profileData.schemaVersion == 1;
    assert sandbox.profileData.network == false;
    assert sandbox.profileData.gpu == false;
    assert sandbox.profileData.tools.dbus == "${pkgs.dbus}/bin/dbus-run-session";
      pkgs.runCommand "dev-sandbox-contract" {} ''
        ${sandbox}/bin/fixture-sandbox --help > launcher-help.txt
        grep -q -- --profile launcher-help.txt
        cp ${sandbox.profile} "$out"
      '';
  }
