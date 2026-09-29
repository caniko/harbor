{
  harbor-rs,
  rustfmtPackage,
}: {pkgs, ...}: {
  imports = [
    harbor-rs.inputs.harbor-meta.treefmtModules.nix
    harbor-rs.inputs.harbor-meta.treefmtModules.toml
    harbor-rs.treefmtModules.rust
  ];
  projectRootFile = "flake.nix";

  # rustfmt comes from harbor-rs's pinned nightly profile, never a floating
  # `nightly.latest`, so template formatting matches the fleet toolchain.
  programs.rustfmt = {
    edition = "2024";
    package = rustfmtPackage;
  };
}
