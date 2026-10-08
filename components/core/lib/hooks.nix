# Shared git-hooks.nix hook fragments. Language-specific hooks (cargo-*,
# ruff-*, …) stay in their owning Harbor and compose these — see
# harbor-rs's `lib.hooks` for the Rust composition.
{
  # `--ci` disables cache reuse and fails when formatting changes the working
  # tree, so an earlier local run cannot hide drift from the commit hook.
  mkTreefmt = {treefmtWrapper}: {
    treefmt = {
      enable = true;
      name = "treefmt";
      package = treefmtWrapper;
      entry = "${treefmtWrapper}/bin/treefmt --ci";
      pass_filenames = false;
    };
  };

  # Full-tree flake evaluation, kept out of the default commit path: it is
  # the slowest hook and CI's job. Developers opt in with `pre-commit run
  # --hook-stage manual nix-flake-check`.
  mkNixFlakeCheck = {pkgs}: {
    nix-flake-check = {
      enable = true;
      name = "nix flake check";
      entry = "nix --extra-experimental-features 'nix-command flakes' flake check --cores 0 --max-jobs auto --no-update-lock-file";
      extraPackages = [pkgs.nix];
      pass_filenames = false;
      stages = ["manual"];
    };
  };
}
