{
  description = "Reusable mdBook + plinth-project documentation infrastructure for Nix flakes";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    flake-utils.url = "github:numtide/flake-utils";

    harbor-meta = {
      url = "git+https://github.com/caniko/harbor-meta.git?ref=feat/shared-timezone-env&rev=44a7c7cbb0cac897bcda2f5ef010dc73362bab9e";
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

        devShells.default = harbor-meta.lib.devShell.mkShell {
          inherit pkgs;
          packages = [pkgs.mdbook];
          extraShellHook = ''
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
