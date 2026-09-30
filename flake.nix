{
  description = "Reusable mdBook + plinth-project documentation infrastructure for Nix flakes";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    flake-utils.url = "github:numtide/flake-utils";

    harbor-meta = {
      url = "git+https://github.com/caniko/harbor-meta.git?ref=trunk&rev=47bc4564cf35fc080e7003e95de18c57dd37e845";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    git-hooks = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    flake-utils,
    harbor-meta,
    treefmt-nix,
    git-hooks,
    ...
  }: let
    lib = import ./lib {
      inherit nixpkgs harbor-meta;
    };
  in
    {
      inherit lib;

      templates.default = {
        path = ./templates/default;
        description = "mdBook docs + plinth-project site with harbor-projects";
      };
    }
    // flake-utils.lib.eachDefaultSystem (
      system: let
        pkgs = import nixpkgs {inherit system;};
        treefmt = treefmt-nix.lib.evalModule pkgs {
          imports = [harbor-meta.treefmtModules.nix harbor-meta.treefmtModules.toml];
          projectRootFile = "flake.nix";
        };
      in {
        formatter = treefmt.config.build.wrapper;

        devShells.default = pkgs.mkShell {
          packages = [pkgs.mdbook];
          shellHook = ''
            echo "Docs preview: mdbook serve templates/default/docs"
          '';
        };

        checks = import ./checks {
          inherit self pkgs treefmt-nix;
          harbor = lib;
          harborMeta = harbor-meta;
        };
      }
    );
}
