{
  description = "Python uv project — powered by harbor-py";

  inputs = {
    harbor-py.url = "github:caniko/harbor-py";
    nixpkgs.follows = "harbor-py/nixpkgs";
    treefmt-nix.follows = "harbor-py/treefmt-nix";
    git-hooks.follows = "harbor-py/git-hooks";
  };

  outputs =
    {
      self,
      nixpkgs,
      harbor-py,
      treefmt-nix,
      git-hooks,
    }:
    let
      py = harbor-py.lib;
      forSystem =
        system:
        let
          pkgs = py.mkPkgs { inherit system; };
          treefmtEval = treefmt-nix.lib.evalModule pkgs (import ./nix/treefmt.nix);
          pre-commit-check = git-hooks.lib.${system}.run {
            src = ./.;
            hooks = import ./nix/pre-commit.nix {
              inherit pkgs;
              treefmtWrapper = treefmtEval.config.build.wrapper;
            };
          };
        in
        {
          inherit treefmtEval pre-commit-check;
          default = py.mkUvDevShell {
            inherit pkgs;
            extraPackages = pre-commit-check.enabledPackages;
            shellHookSuffix = pre-commit-check.shellHook;
          };
        };
    in
    {
      devShells = nixpkgs.lib.genAttrs py.packageSystems (system: {
        default = (forSystem system).default;
      });

      formatter = nixpkgs.lib.genAttrs py.packageSystems (
        system: (forSystem system).treefmtEval.config.build.wrapper
      );

      checks = nixpkgs.lib.genAttrs py.packageSystems (system: {
        formatting = (forSystem system).treefmtEval.config.build.check self;
      });
    };
}
