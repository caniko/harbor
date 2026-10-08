{
  description = "Harbor hub site publisher (isolated from the reusable library flake)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    plinth = {
      url = "git+https://github.com/caniko/plinth.git?ref=refs/heads/trunk&rev=cc1a563deb556b2350c70a71bbd6cda77437144c";
    };
  };

  outputs = {
    nixpkgs,
    plinth,
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
      packages = import ../nix/site.nix {
        inherit pkgs projectSiteLib;
        lib = nixpkgs.lib;
      };
    in {inherit pkgs projectSiteLib packages;};
  in {
    packages = nixpkgs.lib.genAttrs systems (system: (forSystem system).packages);

    apps = nixpkgs.lib.genAttrs systems (system: {
      deploy-pages = (forSystem system).projectSiteLib.mkDeployPagesApp {
        domain = "harbor.tartanoglu.com";
      };
    });

    devShells = nixpkgs.lib.genAttrs systems (system: let
      env = forSystem system;
    in {
      default = env.pkgs.mkShell {
        packages = [plinth.packages.${system}.plinth-project];
        shellHook = ''
          echo "Project site: plinth-project serve --config website/plinth-project.toml --out website/.plinth-project/public"
        '';
      };
    });
  };
}
