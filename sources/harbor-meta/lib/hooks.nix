# Shared git-hooks.nix hook fragments. Language-specific hooks (cargo-*,
# ruff-*, …) stay in their owning Harbor and compose these — see
# harbor-rs's `lib.hooks` for the Rust composition.
{
  # Format the working tree through treefmt; `--fail-on-change` makes an
  # unformatted tree fail instead of silently rewriting staged files.
  mkTreefmt = {treefmtWrapper}: {
    treefmt = {
      enable = true;
      name = "treefmt";
      package = treefmtWrapper;
      entry = "${treefmtWrapper}/bin/treefmt --fail-on-change";
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
