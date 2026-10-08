{
  description = "Bun project — powered by harbor-js";

  inputs = {
    harbor-js.url = "github:caniko/harbor-js";
    nixpkgs.follows = "harbor-js/nixpkgs";
    nixpkgs-darwin.follows = "harbor-js/nixpkgs-darwin";
    treefmt-nix.follows = "harbor-js/treefmt-nix";
    git-hooks.follows = "harbor-js/git-hooks";
  };

  outputs = {
    self,
    nixpkgs,
    nixpkgs-darwin,
    harbor-js,
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
      platformNixpkgs = if system == "x86_64-darwin" then nixpkgs-darwin else nixpkgs;
      pkgs = import platformNixpkgs {
        inherit system;
        overlays = nixpkgs.lib.optionals (system == "x86_64-darwin") [(_: prev: {pnpm_10 = prev.pnpm_10_latest;})];
      };
      treefmtEval = treefmt-nix.lib.evalModule pkgs (import ./nix/treefmt.nix {inherit harbor-js;});
      hooks = import "${git-hooks}/nix" {nixpkgs = platformNixpkgs; inherit system; isFlakes = true;};
      pre-commit-check = hooks.run {
        src = ./.;
        hooks = import ./nix/pre-commit.nix {
          inherit pkgs;
          treefmtWrapper = treefmtEval.config.build.wrapper;
        };
      };
    in {
      inherit treefmtEval pre-commit-check;
      default = harbor-js.lib.mkBunDevShell {
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
