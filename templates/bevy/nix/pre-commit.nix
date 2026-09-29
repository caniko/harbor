# Thin forwarder: the hook composition lives in harbor-rs's lib.hooks so
# every template gets the same treefmt + clippy/audit + flake-check blocks.
{
  pkgs,
  treefmtWrapper,
  rustToolchain ? null,
  harbor-rs,
}:
harbor-rs.lib.hooks.mkRustHooks {
  inherit pkgs treefmtWrapper rustToolchain;
}
