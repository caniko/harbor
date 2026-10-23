# OpenCode TypeScript LSP fragment shared by the Node and Bun shells:
# typescript-language-server on PATH (for any editor) and merged into
# OPENCODE_CONFIG_CONTENT through nix-opencode-lsp, matching the Python shell.
{opencodeLspLib ? null}: {
  pkgs,
  opencodeLsp ? {enable = true;},
}: let
  server = pkgs.typescript-language-server;
in
  if (opencodeLsp.enable or true) && opencodeLspLib != null
  then {
    packages = [server];
    shellHook =
      (opencodeLspLib.mkShell {
        inherit pkgs;
        extraLsp = {
          typescript = {
            command = ["${server}/bin/typescript-language-server" "--stdio"];
            extensions = [".ts" ".tsx" ".mts" ".cts" ".js" ".jsx" ".mjs" ".cjs"];
          };
          # The base config pins nixpkgs' rust-analyzer; resolve it from PATH
          # like harbor-rs's profile, so a composed Rust toolchain's serves.
          rust.command = ["rust-analyzer"];
        };
      }).shellHook;
  }
  else {}
