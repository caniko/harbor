# Native macOS tools use the shared workspace and the same production recipes
# as Linux, without importing Linux-only modules or sandbox checks.
{
  harbor,
  harborRoot,
  nixpkgs,
  nixpkgsDarwin,
  rustOverlay,
}: let
  systems = ["x86_64-darwin" "aarch64-darwin"];
  version = (builtins.fromTOML (builtins.readFile (harborRoot.outPath + "/components/rust/cli/Cargo.toml"))).package.version;
  forSystem = system: let
    pkgs =
      import (
        if system == "x86_64-darwin"
        then nixpkgsDarwin
        else nixpkgs
      ) {
        inherit system;
        overlays = [rustOverlay.overlays.default];
      };
    toolchain = harbor.lib.mkToolchain {
      inherit pkgs;
      toolchainProfile = "nightly";
    };
    arguments = {
      inherit pkgs version;
      inherit (toolchain) craneLib;
      src = harborRoot.outPath;
    };
    cli = import ./harbor-rs-cli.nix arguments;
    ci = import ./harbor-ci.nix arguments;
  in {
    packages = {
      default = cli;
      harbor-rs = cli;
      harbor-ci = ci;
    };
    checks = {
      cli = cli;
      ci = ci;
    };
  };
in {
  packages = nixpkgs.lib.genAttrs systems (system: (forSystem system).packages);
  checks = nixpkgs.lib.genAttrs systems (system: (forSystem system).checks);
}
