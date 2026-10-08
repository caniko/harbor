_: rec {
  normalize = fragment: {
    packages = fragment.packages or [];
    env = fragment.env or {};
    shellHook = fragment.shellHook or "";
  };

  merge = fragments:
    builtins.foldl' (
      acc: fragment: let
        spec = normalize fragment;
      in {
        packages = acc.packages ++ spec.packages;
        env = acc.env // spec.env;
        shellHook = acc.shellHook + spec.shellHook;
      }
    ) {
      packages = [];
      env = {};
      shellHook = "";
    }
    fragments;

  mkShell = {
    pkgs,
    fragments ? [],
    packages ? [],
    env ? {},
    extraShellHook ? "",
    builder ? null,
    mkShellArgs ? {},
  }: let
    spec = merge (
      fragments
      ++ [
        {
          inherit packages env;
          shellHook = extraShellHook;
        }
      ]
    );
    drv =
      if builder != null
      then builder spec
      else
        pkgs.mkShell (
          mkShellArgs
          // {
            inherit (spec) packages env shellHook;
          }
        );
  in
    drv
    // {
      passthru = (drv.passthru or {}) // {devShellSpec = spec;};
    };

  mkPkgConfigEnv = {
    pkgs,
    deps ? [],
  }:
    pkgs.lib.optionalAttrs (deps != []) {
      PKG_CONFIG_PATH = pkgs.lib.makeSearchPathOutput "dev" "lib/pkgconfig" deps;
    };

  # Build a small reusable package/hook pair for making a project CLI
  # available in dev shells without allowing an older PATH entry to win.
  # The predicate contract exposes `HARBOR_PROJECT_CLI` (the command name)
  # and `HARBOR_PROJECT_CLI_PATH` (the resolved path) to
  # `versionCheck.predicate`.
  mkProjectCliShellTools = {
    pkgs,
    package,
    commandName,
    hint ? "",
    versionCheck ? {},
  }: let
    inherit (pkgs) lib;
    expectedPath = "${package}/bin/${commandName}";
    expected = versionCheck.expected or null;
    versionCommand = versionCheck.command or "${commandName} --version";
    predicate = versionCheck.predicate or null;
    hintHook = lib.optionalString (hint != "") ''
      echo ${lib.escapeShellArg hint}
    '';
    expectedHook = lib.optionalString (expected != null) ''
      __harbor_project_cli_version="$(${versionCommand} 2>&1 || true)"
      if ! printf '%s\n' "$__harbor_project_cli_version" | grep -F -- ${lib.escapeShellArg expected} >/dev/null; then
        echo "harbor-meta: ${commandName} version check failed; expected output containing ${expected}" >&2
        echo "$__harbor_project_cli_version" >&2
        return 1 2>/dev/null || exit 1
      fi
    '';
    predicateHook = lib.optionalString (predicate != null) ''
      HARBOR_PROJECT_CLI=${lib.escapeShellArg commandName} \
      HARBOR_PROJECT_CLI_PATH="$__harbor_project_cli_resolved" \
        ${predicate}
    '';
  in {
    packages = [package];
    shellHook = ''
      __harbor_project_cli_resolved="$(command -v ${lib.escapeShellArg commandName} || true)"
      if [ -z "$__harbor_project_cli_resolved" ]; then
        echo "harbor-meta: ${commandName} is not available on PATH" >&2
        return 1 2>/dev/null || exit 1
      fi
      if [ "$__harbor_project_cli_resolved" != ${lib.escapeShellArg expectedPath} ]; then
        echo "harbor-meta: ${commandName} resolved to $__harbor_project_cli_resolved, expected ${expectedPath}" >&2
        return 1 2>/dev/null || exit 1
      fi
      ${expectedHook}
      ${predicateHook}
      ${hintHook}
    '';
  };
}
