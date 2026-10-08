# mkWasmToolchain :: { pkgs, channel ? "stable" }
#                  -> { rustToolchain, craneLib }
#
# Build a craneLib with wasm32-unknown-unknown target.
# Standalone — usable without mkToolchain so projects that only
# need WASM don't pull in cross-compilation targets.
{crane}: {
  pkgs,
  channel ? "stable",
}:
assert pkgs.lib.assertMsg (pkgs ? rust-bin)
"rs-harbor.mkWasmToolchain: requires pkgs with rust-overlay applied (pkgs.rust-bin must exist)";
assert pkgs.lib.assertMsg (builtins.elem channel ["nightly" "stable"])
"rs-harbor.mkWasmToolchain: 'channel' must be \"nightly\" or \"stable\", got \"${channel}\""; let
  channelSet =
    if channel == "nightly"
    then pkgs.rust-bin.nightly
    else pkgs.rust-bin.stable;
  rustToolchain = channelSet.latest.default.override {
    targets = ["wasm32-unknown-unknown"];
  };
  craneLib = (crane.mkLib pkgs).overrideToolchain (_p: rustToolchain);
in {
  inherit rustToolchain craneLib;
}
