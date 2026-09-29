# Rust git-hooks.nix composition: harbor-meta's generic fragments plus the
# cargo toolchain hooks. treefmt is the sole formatter; Rust formatting is
# covered by the configured wrapper, so no separate cargo-fmt hook is emitted.
# Templates forward to `mkRustHooks` instead of
# copying these blocks into every project.
{harbor-meta}: let
  metaHooks =
    if harbor-meta != null
    then harbor-meta.hooks
    else throw "harbor-rs: hook fragments require the harbor-meta flake input";
in {
  mkRustHooks = {
    pkgs,
    treefmtWrapper,
    rustToolchain ? null,
  }:
    metaHooks.mkTreefmt {inherit treefmtWrapper;}
    // {
      cargo-clippy = {
        enable = true;
        name = "cargo clippy";
        entry = "cargo clippy --all-targets --all-features -- --deny warnings";
        extraPackages = pkgs.lib.optional (rustToolchain != null) rustToolchain;
        pass_filenames = false;
      };

      cargo-audit = {
        enable = true;
        name = "cargo audit";
        entry = "cargo audit";
        extraPackages = pkgs.lib.optional (rustToolchain != null) rustToolchain ++ [pkgs.cargo-audit];
        pass_filenames = false;
      };
    }
    // metaHooks.mkNixFlakeCheck {inherit pkgs;};
}
