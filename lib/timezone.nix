# Shared timezone wiring for shells, builders and runtime wrappers.
rec {
  validateName = timeZone:
    if
      builtins.isString timeZone
      && builtins.match "([A-Za-z0-9_+-]+/)*[A-Za-z0-9_+-]+" timeZone != null
    then timeZone
    else throw "harbor-meta: timeZone must be a named zone such as UTC or Europe/Istanbul";

  mkEnvironment = {
    pkgs,
    timeZone ? "UTC",
    tzdata ? pkgs.buildPackages.tzdata,
  }: let
    env = {
      TZ = validateName timeZone;
      TZDIR = "${tzdata}/share/zoneinfo";
    };
    # Validate at shell entry/build time, not by reading a derivation at eval
    # time. libc otherwise silently treats a missing named zone as UTC.
    validationScript = ''
      if [ ! -f "$TZDIR/$TZ" ] || [ "$(${pkgs.buildPackages.coreutils}/bin/head -c 4 "$TZDIR/$TZ" 2>/dev/null)" != TZif ]; then
        echo "harbor-meta: unknown timezone '$TZ' in '$TZDIR'" >&2
        return 1 2>/dev/null || exit 1
      fi
    '';
  in {
    inherit env validationScript;
    packages = [tzdata];
    shellHook = validationScript;
  };

  mkEnv = args: (mkEnvironment args).env;

  # Adapt a pre-existing shell, including shells from older pinned Harbors.
  withShell = {
    pkgs,
    shell,
    timeZone ? (shell.env.TZ or (shell.TZ or (shell.devShellSpec.env.TZ or "UTC"))),
  }: let
    timezone = mkEnvironment {inherit pkgs timeZone;};
  in
    shell.overrideAttrs (old:
      (
        if old ? TZ || old ? TZDIR
        then
          timezone.env
          // (
            if old ? env
            then {env = builtins.removeAttrs old.env ["TZ" "TZDIR"];}
            else {}
          )
        else {env = (old.env or {}) // timezone.env;}
      )
      // {
        shellHook = timezone.validationScript + (old.shellHook or "");
        passthru =
          (old.passthru or {})
          // (
            if (old.passthru or {}) ? devShellSpec
            then {
              devShellSpec =
                old.passthru.devShellSpec
                // {
                  env = old.passthru.devShellSpec.env // timezone.env;
                  shellHook = timezone.validationScript + old.passthru.devShellSpec.shellHook;
                };
            }
            else {}
          );
      });
}
