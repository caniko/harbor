{rustfmtPackage}: {pkgs, ...}: {
  projectRootFile = "flake.nix";
  programs.rustfmt = {
    enable = true;
    edition = "2024";
    package = rustfmtPackage;
  };
  programs.alejandra.enable = true;
  programs.taplo.enable = true;
  programs.ruff-format.enable = true;
  programs.prettier = {
    enable = true;
    package = pkgs.prettier;
    includes = ["*.md" "*.json" "*.yaml" "*.yml" "*.js" "*.ts"];
  };
  # Imported templates and byte-sensitive test data retain their source policy.
  # Simit owns workflow serialization; it verifies those files independently.
  # Submodule components own their formatting (each repository checks it).
  settings.excludes = ["components/core/**" "components/rust/**" "components/python/**" "components/javascript/**" "components/go/**" "components/android/**" "components/database/**" "components/evm/**" "components/solana/**" "components/ntt/**" "components/llm/**" "components/projects/**" "components/cad/**" "components/tex/**" "components/database/tests/oracles/**" "migration/legacy/**" "**/templates/**" "**/tests/fixtures/**" "**/checks/fixtures/**" "components/llm/test/*.json" "components/llm/protocol/execution.json" "components/llm/protocol/conformance.json" "components/llm/protocol/commands.json" "components/llm/src/execution/types.d.ts" "components/llm/src/execution/command-types.d.ts" ".github/workflows/*.yaml" "nix/packages/*/package-lock.json"];
}
