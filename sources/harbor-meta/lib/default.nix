{nixpkgs}: let
  nixLib = nixpkgs.lib;
  opencodeEngine = import ./opencode.nix {lib = nixLib;};
in {
  flake = import ./flake.nix {lib = nixLib;};
  opencode =
    opencodeEngine
    // (import ./opencode-cli.nix {
      lib = nixLib;
      opencode = opencodeEngine;
    });
  packageTests = import ./package-tests.nix {lib = nixLib;};
  hooks = import ./hooks.nix;
  devShell = import ./shell.nix {lib = nixLib;};
  devShellTests = import ./dev-shell-tests.nix {lib = nixLib;};
  templateTests = import ./template-tests.nix {lib = nixLib;};
  treefmtScope = import ./treefmt-scope.nix {lib = nixLib;};
}
