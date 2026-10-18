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
  settings.excludes = ["components/database/tests/oracles/**" "migration/legacy/**" "**/templates/**" "**/tests/fixtures/**" "**/checks/fixtures/**" "components/llm/test/*.json" "components/llm/protocol/execution.json" "components/llm/protocol/conformance.json" "components/llm/src/execution/types.d.ts" ".github/workflows/*.yaml" "nix/packages/*/package-lock.json"];
}
