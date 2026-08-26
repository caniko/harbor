{
  description = "Foundry project powered by harbor-eth";

  inputs = {
    harbor-eth.url = "github:caniko/harbor-eth";
    nixpkgs.follows = "harbor-eth/nixpkgs";
  };

  outputs = {
    harbor-eth,
    nixpkgs,
    ...
  }: let
    systems = ["x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin"];
  in {
    devShells = nixpkgs.lib.genAttrs systems (system: {
      default = harbor-eth.lib.mkEthDevShell {
        pkgs = import nixpkgs {inherit system;};
      };
    });

    formatter = nixpkgs.lib.genAttrs systems (system: (import nixpkgs {inherit system;}).alejandra);
  };
}
