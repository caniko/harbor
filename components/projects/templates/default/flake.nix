{
  description = "My Project docs — powered by harbor-projects";

  inputs = {
    harbor-projects.url = "github:caniko/harbor-projects";

    nixpkgs.follows = "harbor-projects/nixpkgs";
    harbor-meta.follows = "harbor-projects/harbor-meta";
    treefmt-nix.follows = "harbor-projects/treefmt-nix";
  };

  outputs = {
    self,
    nixpkgs,
    harbor-projects,
    harbor-meta,
    treefmt-nix,
    ...
  }: let
    systems = ["x86_64-linux" "aarch64-linux"];
    forSystem = system: let
      pkgs = import nixpkgs {inherit system;};
      packages = import ./nix/docs.nix {
        inherit pkgs;
        harborProjects = harbor-projects.lib;
      };
      treefmt = treefmt-nix.lib.evalModule pkgs {
        imports = [harbor-meta.treefmtModules.nix harbor-meta.treefmtModules.toml];
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
      summary = harbor-projects.lib.mkSummaryCheck {
        pkgs = env.pkgs;
        src = ./docs;
      };
      formatting = env.treefmt.config.build.check self;
    });

    formatter = nixpkgs.lib.genAttrs systems (system: (forSystem system).treefmt.config.build.wrapper);
  };
}
