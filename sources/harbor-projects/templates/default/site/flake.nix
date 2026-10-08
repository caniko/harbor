{
  description = "My Project site publisher (isolated from the reusable docs flake)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    harbor-projects.url = "github:caniko/harbor-projects";

    plinth = {
      url = "git+https://github.com/caniko/plinth.git?ref=refs/heads/trunk";
    };
  };

  outputs = {
    nixpkgs,
    harbor-projects,
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
        harborProjects = harbor-projects.lib;
      };
    in {inherit pkgs projectSiteLib packages;};
  in {
    packages = nixpkgs.lib.genAttrs systems (system: (forSystem system).packages);

    devShells = nixpkgs.lib.genAttrs systems (system: let
      env = forSystem system;
    in {
      default = harbor-projects.lib.mkDocsDevShell {
        pkgs = env.pkgs;
        plinthProject = plinth.packages.${system}.plinth-project;
      };
    });
  };
}
