{
  description = "harbor-rs project site publisher (isolated from the reusable library flake)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    plinth = {
      # NOTE: no nixpkgs follows here. Plinth pins the newest nixpkgs whose
      # dioxus-cli matches its Cargo.lock and asserts the match at eval;
      # following unstable in breaks evaluation (hub site uses the same
      # policy). Revisit when plinth migrates to dioxus 0.7.10.
      url = "git+https://github.com/caniko/plinth.git?ref=refs/heads/trunk";
      inputs.harbor-rs.follows = "harbor-rs";
    };

    harbor-rs = {
      url = "git+https://github.com/caniko/harbor-rs.git?ref=trunk&rev=720a933c1d7a9ceac7caccc494425f2a3d54a91c";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    harbor-projects = {
      url = "github:caniko/harbor-projects";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    nixpkgs,
    plinth,
    harbor-rs,
    harbor-projects,
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
        harborDocs = harbor-projects.lib;
      };
    in {inherit pkgs projectSiteLib packages;};
  in {
    packages = nixpkgs.lib.genAttrs systems (system: (forSystem system).packages);

    apps = nixpkgs.lib.genAttrs systems (system: {
      deploy-pages = (forSystem system).projectSiteLib.mkDeployPagesApp {
        domain = "harbor-rs.tartanoglu.com";
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
