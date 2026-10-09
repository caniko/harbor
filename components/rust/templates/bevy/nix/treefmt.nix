{
  harbor,
  rustfmtPackage,
}: {pkgs, ...}: {
  imports = [
    harbor.treefmtModules.core-nix
    harbor.treefmtModules.core-toml
    harbor.treefmtModules.rust-rust
  ];
  projectRootFile = "flake.nix";

  # rustfmt comes from harbor-rs's pinned nightly profile, never a floating
  # `nightly.latest`, so template formatting matches the fleet toolchain.
  programs.rustfmt = {
    edition = "2024";
    package = rustfmtPackage;
  };
}
