# treefmt-nix composition for harbor-meta itself.
#
# Single source of truth for two consumers: the flake's `formatter` output
# (imported by flake.nix) and the workspace format fleet —
# `canix workspace format` discovers a project by this exact path. Keep the
# formatter set in sync with the exports table in docs/treefmt.md.
{...}: {
  imports = [./treefmt/nix.nix ./treefmt/toml.nix];
  projectRootFile = "flake.nix";
}
