{
  description = "My Project site publisher (isolated from the reusable docs flake)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    harbor.url = "git+https://github.com/caniko/harbor.git?ref=feat/harbor-monorepo-components&rev=7d99eb50c52d0a941e2996b97c469b32a7657ef4";

    plinth = {
      url = "git+https://github.com/caniko/plinth.git?ref=refs/heads/trunk";
    };
  };

  outputs = {
    nixpkgs,
    harbor,
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
        harborProjects = harbor.lib.docs;
      };
    in {inherit pkgs projectSiteLib packages;};
  in {
    packages = nixpkgs.lib.genAttrs systems (system: (forSystem system).packages);

    devShells = nixpkgs.lib.genAttrs systems (system: let
      env = forSystem system;
    in {
      default = harbor.lib.docs.mkDocsDevShell {
        pkgs = env.pkgs;
        plinthProject = plinth.packages.${system}.plinth-project;
      };
    });
  };
}
