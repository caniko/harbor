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
  settings.excludes = ["migration/legacy/**" "**/templates/**" "**/tests/fixtures/**" "**/checks/fixtures/**" "components/llm/test/*.json" ".github/workflows/*.yaml"];
}
