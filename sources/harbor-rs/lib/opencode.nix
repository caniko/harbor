# Rust profile for the harbor-meta opencode engine. This is the only place
# harbor-rs introduces anything language-specific: the `rust` LSP block,
# the signals that select it, and the packages providing the servers.
# `rust-analyzer` itself comes from the crane toolchain (overrideToolchain
# puts it on PATH in every devShell), not from the profile packages.
{harbor-meta}: let
  engine =
    if harbor-meta != null
    then harbor-meta.opencode
    else throw "harbor-rs: opencode helpers require the harbor-meta flake input";
in rec {
  rustLsp = {
    rust.command = ["rust-analyzer"];
    nixd.command = ["nixd"];
    taplo = {
      command = ["taplo" "lsp" "stdio"];
      extensions = [".toml"];
    };
  };

  profiles = {
    rust = {
      lsp = rustLsp;
      detect = {
        files = ["Cargo.toml"];
        flakeMarkers = ["harbor-rs" "rs-harbor" "mkDevShell" "mkDevShells"];
      };
      packages = pkgs: [pkgs.nixd pkgs.taplo];
    };
  };

  # `harbor-opencode` bound to the rust registry: `--kind rust` — and
  # `--kind detect` on Rust projects — renders the rust LSP block, while
  # the engine's policy-only fallback covers everything else.
  mkCli = {pkgs}: engine.mkCli {inherit pkgs profiles;};
}
