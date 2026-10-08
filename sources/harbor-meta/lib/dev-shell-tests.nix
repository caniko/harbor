{lib}: {
  mkCheck = {
    pkgs,
    name,
    shell,
    commands ? [],
    env ? {},
    hookContains ? [],
    runHook ? false,
  }: let
    spec = shell.passthru.devShellSpec or null;
    shellEnv =
      if spec != null
      then spec.env
      else (shell.env or {});
    hook =
      if spec != null
      then spec.shellHook
      else (shell.shellHook or "");
    envOk = lib.all (
      name:
        (shellEnv.${name} or shell.${name} or null) == env.${name}
    ) (builtins.attrNames env);
    hooksOk = lib.all (needle: lib.hasInfix needle hook) hookContains;
  in
    assert lib.assertMsg ((shell.type or null) == "derivation")
    "devShellTests: shell is not a derivation";
    assert lib.assertMsg envOk "devShellTests: env mismatch";
    assert lib.assertMsg hooksOk "devShellTests: shellHook missing expected text";
      pkgs.runCommand name {
        nativeBuildInputs = shell.nativeBuildInputs or [];
      } ''
        ${lib.concatMapStrings (command: ''
            command -v ${lib.escapeShellArg command}
          '')
          commands}
        ${lib.optionalString runHook ''
          export HOME="$TMPDIR"
          export XDG_CACHE_HOME="$TMPDIR"
          ${hook}
        ''}
        mkdir -p "$out"
        echo ok > "$out/result"
      '';
}
