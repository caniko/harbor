{
  description = "My Project docs — powered by harbor-projects";

  inputs = {
    harbor.url = "git+https://github.com/caniko/harbor.git?ref=feat/harbor-monorepo-components&rev=7d99eb50c52d0a941e2996b97c469b32a7657ef4";

    nixpkgs.follows = "harbor/nixpkgs";
    treefmt-nix.follows = "harbor/treefmt-nix";
  };

  outputs = {
    self,
    nixpkgs,
    harbor,
    treefmt-nix,
    ...
  }: let
    systems = ["x86_64-linux" "aarch64-linux"];
    forSystem = system: let
      pkgs = import nixpkgs {inherit system;};
      packages = import ./nix/docs.nix {
        inherit pkgs;
        harborProjects = harbor.lib.docs;
      };
      treefmt = treefmt-nix.lib.evalModule pkgs {
        imports = [harbor.treefmtModules.core-nix harbor.treefmtModules.core-toml];
        projectRootFile = "flake.nix";
      };
    in {inherit pkgs packages treefmt;};
  in {
    packages = nixpkgs.lib.genAttrs systems (system: {
      docs = (forSystem system).packages.docs;
    });

    devShells = nixpkgs.lib.genAttrs systems (system: let
      env = forSystem system;
    in {
      default = env.pkgs.mkShell {
        packages = [env.pkgs.mdbook];
        shellHook = ''
          echo "Documentation: mdbook serve docs"
        '';
      };
    });

    checks = nixpkgs.lib.genAttrs systems (system: let
      env = forSystem system;
    in {
      docs = env.packages.docs;
      summary = harbor.lib.docs.mkSummaryCheck {
        pkgs = env.pkgs;
        src = ./docs;
      };
      formatting = env.treefmt.config.build.check self;
    });

    formatter = nixpkgs.lib.genAttrs systems (system: (forSystem system).treefmt.config.build.wrapper);
  };
}
