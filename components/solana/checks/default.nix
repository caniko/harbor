{
  pkgs,
  self,
  nixpkgs,
  meta,
}: {
  dev-shell = meta.devShellTests.mkCheck {
    inherit pkgs;
    name = "harbor-sol-dev-shell";
    shell = self.devShells.${pkgs.stdenv.hostPlatform.system}.default;
    commands = ["anchor" "cargo" "cargo-build-sbf" "rustc" "solana" "solana-test-validator"];
    env = {
      ANCHOR_VERSION = pkgs.anchor.version;
      SOLANA_VERSION = pkgs.solana-cli.version;
    };
  };

  template-default = meta.templateTests.mkCheck {
    inherit pkgs;
    system = pkgs.stdenv.hostPlatform.system;
    flakeNix = ../templates/default/flake.nix;
    inputs = {
      harbor-sol = self;
      inherit nixpkgs;
      inherit (self.inputs) treefmt-nix;
    };
    requiredFiles = [
      "flake.nix"
      "Anchor.toml"
      "Cargo.toml"
      "Cargo.lock"
      "programs/counter/Cargo.toml"
      "programs/counter/src/lib.rs"
    ];
    requiredInputs = ["harbor-sol"];
    commands = ["anchor" "cargo" "cargo-build-sbf" "solana" "solana-test-validator"];
    env = {
      ANCHOR_VERSION = pkgs.anchor.version;
      SOLANA_VERSION = pkgs.solana-cli.version;
    };
    inherit (meta) devShellTests;
  };

  tool-versions =
    pkgs.runCommand "harbor-sol-tool-versions" {
      nativeBuildInputs = [pkgs.anchor self.packages.${pkgs.stdenv.hostPlatform.system}.cargo-build-sbf pkgs.solana-cli];
    } ''
      anchor --version
      cargo-build-sbf --version
        solana --version
        solana-test-validator --version
        mkdir -p "$out"
    '';
}
