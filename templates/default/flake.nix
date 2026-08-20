{
  description = "Bun project — powered by js-harbor";

  inputs = {
    js-harbor.url = "github:caniko/js-harbor";
    nixpkgs.follows = "js-harbor/nixpkgs";
    treefmt-nix.follows = "js-harbor/treefmt-nix";
    git-hooks.follows = "js-harbor/git-hooks";
  };

  outputs = {
    self,
    nixpkgs,
    js-harbor,
    treefmt-nix,
    git-hooks,
  }: let
    systems = [
      "x86_64-linux"
      "aarch64-linux"
      "x86_64-darwin"
      "aarch64-darwin"
    ];
    forSystem = system: let
      pkgs = import nixpkgs {inherit system;};
      treefmtEval = treefmt-nix.lib.evalModule pkgs (import ./nix/treefmt.nix);
      pre-commit-check = git-hooks.lib.${system}.run {
        src = ./.;
        hooks = import ./nix/pre-commit.nix {
          inherit pkgs;
          treefmtWrapper = treefmtEval.config.build.wrapper;
        };
      };
    in {
      inherit treefmtEval pre-commit-check;
      default = js-harbor.lib.mkBunDevShell {
        inherit pkgs;
        packageJson = ./package.json;
        extraPackages = pre-commit-check.enabledPackages;
        extraShellHook = pre-commit-check.shellHook;
      };
    };
  in {
    devShells = nixpkgs.lib.genAttrs systems (system: {
      default = (forSystem system).default;
    });

    formatter = nixpkgs.lib.genAttrs systems (
      system: (forSystem system).treefmtEval.config.build.wrapper
    );

    checks = nixpkgs.lib.genAttrs systems (system: {
      formatting = (forSystem system).treefmtEval.config.build.check self;
    });
  };
}
