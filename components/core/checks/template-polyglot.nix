# The polyglot template's composed shell provides every language toolchain,
# and its Rust and TypeScript samples pass offline. Python's .venv sync needs
# the network, so pytest stays a consumer check; the shell hook is asserted.
{
  pkgs,
  system,
  harborRoot,
  treefmt-nix,
  nixpkgs,
  templateTests,
  devShellTests,
}: let
  root = ../templates/polyglot;
  inputs = {
    harbor = harborRoot;
    inherit nixpkgs treefmt-nix;
  };
  shell =
    (templateTests.eval {
      flakeNix = root + "/flake.nix";
      inherit inputs;
    }).devShells.${
      system
    }.default;
in {
  template-polyglot = templateTests.mkCheck {
    inherit pkgs system inputs devShellTests;
    flakeNix = root + "/flake.nix";
    requiredFiles = [
      "flake.nix"
      "flake.lock"
      ".envrc"
      ".vscode/settings.json"
      ".vscode/rust-analyzer"
      "pyproject.toml"
      "uv.lock"
      "Cargo.toml"
      "Cargo.lock"
      "package.json"
      "package-lock.json"
      "tsconfig.json"
    ];
    requiredInputs = ["harbor" "treefmt-nix"];
    commands = ["cargo" "rustc" "rust-analyzer" "cargo-clippy" "rustfmt" "python" "uv" "node" "npm" "treefmt"];
    hookContains = ["uv sync" "RS_HARBOR_CARGO_HOME"];
  };

  template-polyglot-samples =
    pkgs.runCommand "harbor-template-polyglot-samples" {
      inherit (shell) nativeBuildInputs;
    } ''
        cp -r ${root} project
        chmod -R u+w project
        cd project
        export HOME="$TMPDIR/home" CARGO_HOME="$TMPDIR/cargo"
        if rustc --version | grep -q nightly; then
          echo 'polyglot template resolved a nightly rustc' >&2
          exit 1
        fi
        cargo test --offline --quiet
      # VS Code starts rust-analyzer through this direnv wrapper (settings.json).
      test -x .vscode/rust-analyzer
      grep -q '"rust-analyzer.server.path": "''${workspaceFolder}/.vscode/rust-analyzer"' .vscode/settings.json
        node --test
        touch "$out"
    '';
}
