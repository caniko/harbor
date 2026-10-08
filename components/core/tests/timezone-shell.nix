# Adapter regression without derivations; also used by the native flake check.
{
  timezone ? import ../lib/timezone.nix,
  pkgs ? {
    buildPackages = {
      tzdata = "/fixture/tzdata";
      coreutils = "/fixture/coreutils";
    };
  },
}: let
  shell = attrs: attrs // {overrideAttrs = update: shell (attrs // update attrs);};
  adapt = attrs:
    timezone.withShell {
      inherit pkgs;
      shell = shell attrs;
      timeZone = "Europe/Istanbul";
    };
  spec = {
    env = {
      TZ = "UTC";
      TZDIR = "/custom/zoneinfo";
      UNRELATED = "spec";
    };
    shellHook = "echo original\n";
  };
  topLevel = adapt {
    TZ = "UTC";
    TZDIR = "/custom/zoneinfo";
    env.UNRELATED = "keep";
    passthru.devShellSpec = spec;
    shellHook = spec.shellHook;
  };
  nested = adapt {
    env = spec.env;
    passthru.devShellSpec = spec;
    shellHook = spec.shellHook;
  };
  mixed = adapt {
    TZ = "UTC";
    env = spec.env;
  };
  default = adapt {};
  repeated = timezone.withShell {
    inherit pkgs;
    shell = nested;
  };
  retargeted = timezone.withShell {
    inherit pkgs;
    shell = repeated;
    timeZone = "UTC";
  };
  legacy = adapt {
    env = spec.env;
    shellHook = (timezone.mkEnvironment {inherit pkgs;}).validationScript + spec.shellHook;
  };
  # mkShell can attach metadata after the original overrideAttrs closure exists.
  attached = timezone.withShell {
    inherit pkgs;
    shell = (shell {env = spec.env;}) // {passthru.devShellSpec = spec;};
    timeZone = "Europe/Istanbul";
  };
  changedHook = adapt {
    env = spec.env;
    passthru.devShellSpec = spec;
    shellHook = "echo changed\n";
  };
  metadataOnly = timezone.withShell {
    inherit pkgs;
    shell = (shell {}) // {passthru.devShellSpec = spec // {env = spec.env // {TZ = "Europe/Istanbul";};};};
  };
in
  assert topLevel.TZ == "Europe/Istanbul";
  assert topLevel.TZDIR == "/custom/zoneinfo";
  assert !(topLevel.env ? TZ || topLevel.env ? TZDIR);
  assert topLevel.env.UNRELATED == "keep";
  assert nested.env.TZ == "Europe/Istanbul";
  assert nested.env.TZDIR == "/custom/zoneinfo";
  assert nested.env.UNRELATED == "spec";
  assert mixed.TZDIR == "/custom/zoneinfo" && !(mixed.env ? TZDIR);
  assert default.env.TZDIR == "${pkgs.buildPackages.tzdata}/share/zoneinfo";
  assert repeated.env == nested.env;
  assert repeated.shellHook == nested.shellHook;
  assert retargeted.shellHook == (timezone.mkEnvironment {inherit pkgs;}).shellHook + spec.shellHook;
  assert legacy.shellHook == nested.shellHook;
  assert attached.passthru.devShellSpec.env == nested.passthru.devShellSpec.env;
  assert topLevel.passthru.devShellSpec.env.UNRELATED == topLevel.env.UNRELATED;
  assert changedHook.passthru.devShellSpec.shellHook == changedHook.shellHook;
  assert metadataOnly.env.TZ == "Europe/Istanbul";
  assert builtins.all (value:
    value.passthru.devShellSpec.env.TZDIR
    == "/custom/zoneinfo"
    && value.passthru.devShellSpec.env.TZ == "Europe/Istanbul"
    && value.shellHook
    == (timezone.mkEnvironment {
      inherit pkgs;
      timeZone = "Europe/Istanbul";
    }).shellHook
    + spec.shellHook)
  [topLevel nested]; true
