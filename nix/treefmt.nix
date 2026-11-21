# Root policy, composed from the components' own treefmt modules so the
# superproject formats with exactly the harbor helpers its consumers use.
{rustfmtPackage}: {pkgs, ...}: {
  imports = [
    ../components/core/nix/treefmt/nix.nix
    ../components/core/nix/treefmt/toml.nix
    ../components/core/nix/treefmt/markdown.nix
    ../components/core/nix/treefmt/shell.nix
    ../components/core/nix/treefmt/svg.nix
    ../components/rust/nix/treefmt/rust.nix
    ../components/python/nix/treefmt/python.nix
    ../components/javascript/nix/treefmt/javascript.nix
  ];
  projectRootFile = "flake.nix";
  programs.rustfmt.package = rustfmtPackage;
  # The root also formats its workflow-adjacent YAML.
  programs.prettier = {
    package = pkgs.prettier;
    includes = ["*.yaml" "*.yml"];
  };
  # Imported templates and byte-sensitive test data retain their source policy.
  # Simit owns workflow serialization; it verifies those files independently.
  # Submodule components own their formatting: each checks its own
  # nix/treefmt.nix, inside harbor as well as standalone.
  settings.excludes = ["components/core/**" "components/rust/**" "components/python/**" "components/javascript/**" "components/go/**" "components/android/**" "components/database/**" "components/evm/**" "components/solana/**" "components/ntt/**" "components/llm/**" "components/projects/**" "components/cad/**" "components/tex/**" "components/game/**" "components/mcp/**" "migration/legacy/**" "**/templates/**" "**/tests/fixtures/**" "**/checks/fixtures/**" ".github/workflows/*.yaml" "nix/packages/*/package-lock.json"];
}
