{
  pkgs,
  self,
  nixpkgs,
  nixpkgs-darwin,
  meta,
}: {
  dev-shell = meta.devShellTests.mkCheck {
    inherit pkgs;
    name = "harbor-eth-dev-shell";
    shell = self.devShells.${pkgs.stdenv.hostPlatform.system}.default;
    commands = ["anvil" "cast" "chisel" "forge" "solc"];
    env = {
      FOUNDRY_SOLC = "${pkgs.solc}/bin/solc";
      FOUNDRY_VERSION = pkgs.foundry.version;
      SOLC_VERSION = pkgs.solc.version;
    };
  };

  template-default = meta.templateTests.mkCheck {
    inherit pkgs;
    system = pkgs.stdenv.hostPlatform.system;
    flakeNix = ../templates/default/flake.nix;
    inputs = {
      harbor = self.inputs.harborRoot;
      inherit nixpkgs nixpkgs-darwin;
      inherit (self.inputs) treefmt-nix;
    };
    requiredFiles = ["flake.nix" "foundry.toml" "src/Counter.sol" "test/Counter.t.sol"];
    requiredInputs = ["harbor" "nixpkgs-darwin"];
    commands = ["anvil" "forge" "solc"];
    env = {
      FOUNDRY_SOLC = "${pkgs.solc}/bin/solc";
      FOUNDRY_VERSION = pkgs.foundry.version;
      SOLC_VERSION = pkgs.solc.version;
    };
    inherit (meta) devShellTests;
  };

  foundry-template =
    pkgs.runCommand "harbor-eth-foundry-template" {
      nativeBuildInputs = [pkgs.foundry pkgs.solc];
    } ''
          cp -R ${../templates/default} project
          chmod -R u+w project
        cd project
        export HOME="$TMPDIR"
        export FOUNDRY_SOLC=${pkgs.solc}/bin/solc
      forge test --offline
          mkdir -p "$out"
    '';
}
