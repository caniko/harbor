{
  pkgs,
  nixDirenv,
}: let
  inherit (pkgs) lib;
  # Patching must retain a base package's own patches and fixup hooks.
  existingPatch = pkgs.writeText "existing-direnv.patch" ''
    --- a/share/nix-direnv/direnvrc
    +++ b/share/nix-direnv/direnvrc
    @@ -1,2 +1,3 @@
     # -*- mode: sh -*-
     # shellcheck shell=bash
    +# Existing consumer patch retained.
  '';
  composed = import ../nix/direnv/package.nix {
    inherit pkgs;
    nix-direnv = pkgs.nix-direnv.overrideAttrs (old: {
      patches = (old.patches or []) ++ [existingPatch];
      postFixup = (old.postFixup or "") + ''touch "$out/existing-hook"'';
    });
  };
  direnvrc = "${nixDirenv}/share/nix-direnv/direnvrc";
  bash = lib.getExe pkgs.bash;
  # A stdenv-free shell with one input, so print-dev-env must build it.
  fixture = pkgs.writeTextDir "flake.nix" ''
    {
      outputs = _: let
        system = "${pkgs.stdenv.hostPlatform.system}";
        input = derivation {
          name = "direnv-input-v1";
          inherit system;
          builder = "${bash}";
          args = ["-c" "${pkgs.coreutils}/bin/mkdir $out"];
        };
      in {
        devShells.''${system}.default = derivation {
          name = "direnv-fixture-shell";
          inherit system;
          builder = "${bash}";
          args = ["-c" "exit 1"];
          outputs = ["out"];
          INPUT = input;
        };
      };
    }
  '';
in {
  # The sandbox has no user systemd, so this covers the inline fallback.
  nix-direnv-roots = assert lib.elem existingPatch composed.patches;
    pkgs.runCommand "nix-direnv-roots" {
      nativeBuildInputs = [pkgs.bash pkgs.nix pkgs.coreutils];
    } ''
      test -f ${composed}/existing-hook
      mkdir negative positive
      # The unpatched installed output must fail this same regression harness.
      if (cd negative; bash ${./direnv-roots.sh} ${pkgs.nix-direnv}/share/nix-direnv/direnvrc); then
        echo 'Faulty renewal unexpectedly passed' >&2
        exit 1
      fi
      echo 'RED: unpatched installed nix-direnv rejected'
      (cd positive; bash ${./direnv-roots.sh} ${direnvrc})
      touch "$out"
    '';

  nix-direnv-runtime = pkgs.testers.runNixOSTest {
    name = "nix-direnv-runtime";
    nodes.machine = {
      environment.systemPackages = [pkgs.bash pkgs.coreutils pkgs.direnv pkgs.gnused pkgs.gnugrep];
      virtualisation.additionalPaths = [fixture pkgs.bash pkgs.coreutils];
      # With one vCPU and 1 GiB, Nix in the VM intermittently SIGKILLs builders.
      virtualisation.cores = 2;
      virtualisation.memorySize = 2048;
      nix.settings = {
        experimental-features = ["nix-command" "flakes"];
        # The fixture names its builders by store path, without input context.
        sandbox = false;
        substituters = lib.mkForce [];
      };
      users.users.alice = {
        isNormalUser = true;
        linger = true;
      };
    };
    testScript = let
      as = user: phase:
        if user == "root"
        then "bash ${./direnv-detached.sh} ${direnvrc} ${fixture} ${phase}"
        else "su -l alice -c 'XDG_RUNTIME_DIR=/run/user/1000 bash ${./direnv-detached.sh} ${direnvrc} ${fixture} ${phase}'";
    in ''
      start_all()
      machine.wait_for_unit("multi-user.target")
      machine.succeed("mkdir -p /run/direnv-fixture")
      machine.succeed("cd /run/direnv-fixture; REAL_ROOTS=1 NIX_ROOT_BINARY=${pkgs.nix}/bin/nix NIX_STORE_BINARY=${pkgs.nix}/bin/nix-store bash ${./direnv-roots.sh} ${direnvrc}")
      machine.wait_for_unit("user@1000.service")
      machine.succeed("${as "alice" "interrupt"}")
      # The finished realization is rooted before any activation publishes it.
      machine.fail("nix-store --delete \"$(ls -d /nix/store/*-direnv-input-v1)\"")
      machine.succeed("test -d \"$(ls -d /nix/store/*-direnv-input-v1)\"")
      machine.succeed("${as "alice" "renew"}")
      machine.succeed("${as "alice" "cached"}")
      machine.succeed("${as "alice" "concurrent"}")
      # Root has no user manager here: the plain inline call remains.
      machine.succeed("${as "root" "fallback"}")
    '';
  };
}
