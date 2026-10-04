{
  pkgs,
  lib,
}: let
  timezone = lib.timezone;
  utc = timezone.mkEnvironment {inherit pkgs;};
  istanbul = timezone.mkEnvironment {
    inherit pkgs;
    timeZone = "Europe/Istanbul";
  };
  shell = lib.devShell.mkShell {
    inherit pkgs;
    timeZone = "Europe/Istanbul";
  };
  overridden = lib.devShell.mkShell {
    inherit pkgs;
    env.TZ = "America/New_York";
  };
in
  assert utc.env.TZ == "UTC";
  assert istanbul.env.TZDIR == "${pkgs.buildPackages.tzdata}/share/zoneinfo";
  assert shell.devShellSpec.env.TZ == "Europe/Istanbul";
  assert overridden.devShellSpec.env.TZ == "America/New_York";
  assert builtins.all (name: !(builtins.tryEval (timezone.validateName name)).success)
  ["" "../UTC" "/etc/localtime" "Europe//Istanbul" "UTC\n" ":UTC" "UTC;false"];
    pkgs.runCommand "harbor-meta-timezone" {} ''
      set -euo pipefail
      export TZDIR=${pkgs.lib.escapeShellArg utc.env.TZDIR}
      export TZ=UTC
      ${utc.validationScript}
      test "$(date -d @946684800 +%H:%M:%z)" = "00:00:+0000"
      export TZ=Europe/Istanbul
      ${istanbul.validationScript}
      test "$(date -d @946684800 +%H:%M:%z)" = "02:00:+0200"
      test "$(date -d @1593561600 +%H:%M:%z)" = "03:00:+0300"
      export TZ=America/New_York
      ${utc.validationScript}
      test "$(date -d @946684800 +%H:%M:%z)" = "19:00:-0500"
      test "$(date -d @1593561600 +%H:%M:%z)" = "20:00:-0400"
      export TZ=Missing/Zone
      if ( ${utc.validationScript} ); then
        echo "missing timezone unexpectedly accepted" >&2
        exit 1
      fi
      export TZ=leapseconds
      if ( ${utc.validationScript} ); then
        echo "non-zone tzdata file unexpectedly accepted" >&2
        exit 1
      fi
      touch "$out"
    ''
