# Exercise actual mkShell overrides and Harbor's attached shell metadata.
{
  pkgs,
  timezone ? import ../lib/timezone.nix,
}: let
  database = "${pkgs.buildPackages.tzdata}/share/zoneinfo/.";
  spec = {
    env = {
      TZ = "UTC";
      TZDIR = database;
    };
    shellHook = ''
      test "$TZ" = Europe/Istanbul
      test "$TZDIR" = ${pkgs.lib.escapeShellArg database}
      test "$(date -d @1593561600 +%H:%M:%z)" = "03:00:+0300"
    '';
  };
  topLevel = timezone.withShell {
    inherit pkgs;
    timeZone = "Europe/Istanbul";
    shell = pkgs.mkShell (spec.env // {inherit (spec) shellHook;});
  };
  nested = timezone.withShell {
    inherit pkgs;
    timeZone = "Europe/Istanbul";
    shell = (import ../lib/shell.nix {}).mkShell {
      inherit pkgs;
      inherit (spec) env;
      extraShellHook = spec.shellHook;
    };
  };
  direct = (import ../lib/shell.nix {}).mkShell {
    inherit pkgs;
    timeZone = "Europe/Istanbul";
    env.TZDIR = database;
    extraShellHook = spec.shellHook;
  };
  repeated = timezone.withShell {
    inherit pkgs;
    shell = nested;
  };
in
  assert topLevel.TZDIR == database;
  assert nested.TZDIR == database;
  assert nested.passthru.devShellSpec.env.TZDIR == database;
  assert nested.passthru.devShellSpec.env.TZ == "Europe/Istanbul";
  assert repeated.shellHook == nested.shellHook;
  assert nested.passthru.devShellSpec.shellHook == nested.shellHook; {
    topLevel = {
      inherit (topLevel) drvPath shellHook;
      env = {inherit (topLevel) TZ TZDIR;};
    };
    nested = {
      inherit (nested) drvPath shellHook;
      env = {inherit (nested) TZ TZDIR;};
    };
    direct = {
      inherit (direct) drvPath shellHook;
      env = {inherit (direct) TZ TZDIR;};
    };
    repeated = {
      inherit (repeated) drvPath shellHook;
      env = {inherit (repeated) TZ TZDIR;};
    };
  }
