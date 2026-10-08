{
  description = "Reusable Python uv2nix and pyproject-nix infrastructure for Nix flakes";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    flake-utils.url = "github:numtide/flake-utils";

    pyproject-nix = {
      url = "github:pyproject-nix/pyproject.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    uv2nix = {
      url = "github:pyproject-nix/uv2nix";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.pyproject-nix.follows = "pyproject-nix";
    };

    pyproject-build-systems = {
      url = "github:pyproject-nix/build-system-pkgs";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.pyproject-nix.follows = "pyproject-nix";
      inputs.uv2nix.follows = "uv2nix";
    };

    harbor-meta = {
      url = "git+https://github.com/caniko/harbor-meta.git?ref=trunk";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    meta-harbor.follows = "harbor-meta";

    nix-opencode-lsp = {
      url = "git+https://github.com/caniko/nix-opencode-lsp.git?ref=trunk";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.flake-utils.follows = "flake-utils";
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
    pyproject-nix,
    uv2nix,
    pyproject-build-systems,
    harbor-meta,
    nix-opencode-lsp,
    treefmt-nix,
    git-hooks,
    ...
  }: let
    lib = import ./lib {
      inherit
        nixpkgs
        pyproject-nix
        uv2nix
        pyproject-build-systems
        harbor-meta
        ;
      opencodeLspLib = nix-opencode-lsp.lib;
    };
  in
    {
        inherit lib;

        templates.default = {
          path = ./templates/default;
          description = "Python uv project with harbor-py";
        };
      }
    // flake-utils.lib.eachDefaultSystem (
      system: let
        pkgs = lib.mkPkgs {inherit system;};
      in {
        formatter = pkgs.writeShellApplication {
          name = "harbor-py-fmt";
          runtimeInputs = [pkgs.nixfmt];
          text = ''
            if [ "$#" -eq 0 ]; then
              find . -name '*.nix' -print0 | xargs -0 nixfmt
            else
              exec nixfmt "$@"
            fi
          '';
        };

        devShells = {
          opencode-lsp-python = nix-opencode-lsp.lib.mkShell {
            inherit pkgs;
            profiles = ["python"];
          };
          default = self.devShells.${system}.opencode-lsp-python;
        };

        checks = import ./checks {
          inherit self pkgs system nixpkgs treefmt-nix git-hooks;
          harbor = lib;
          meta = harbor-meta.lib;
        };
      }
    );
}
