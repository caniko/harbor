{
  description = "rs-harbor project site publisher (isolated from the reusable library flake)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    plinth = {
      url = "git+https://codeberg.org/caniko/plinth.git?ref=refs/heads/trunk";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.harbor-rs.follows = "harbor-rs";
    };

    harbor-rs = {
      url = "git+https://github.com/caniko/harbor-rs.git?ref=trunk&rev=9bfa8bdb0ecb22d7bc11448665f7fbaebae7a759";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    nixpkgs,
    plinth,
    harbor-rs,
    ...
  }: let
    systems = ["x86_64-linux" "aarch64-linux"];

    forSystem = system: let
      pkgs = import nixpkgs {inherit system;};
      projectSiteLib = import "${plinth}/nix/project-site.nix" {
        inherit pkgs;
        lib = nixpkgs.lib;
        plinthProject = plinth.packages.${system}.plinth-project;
      };
      packages = import "${harbor-rs}/nix/site.nix" {
        inherit pkgs projectSiteLib;
        lib = nixpkgs.lib;
      };
    in {inherit pkgs projectSiteLib packages;};
  in {
    packages = nixpkgs.lib.genAttrs systems (system: (forSystem system).packages);

    apps = nixpkgs.lib.genAttrs systems (system: {
      deploy-pages = (forSystem system).projectSiteLib.mkDeployPagesApp {
        domain = "rs-harbor.tartanoglu.com";
      };
    });

    devShells = nixpkgs.lib.genAttrs systems (system: let
      env = forSystem system;
    in {
      docs = env.pkgs.mkShell {
        packages = [
          env.pkgs.mdbook
          plinth.packages.${system}.plinth-project
        ];
        shellHook = ''
          echo "Project site: plinth-project serve --config website/plinth-project.toml --out website/.plinth-project/public"
          echo "Documentation: mdbook serve docs"
        '';
      };
    });
  };
}
