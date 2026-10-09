{
  description = "LaTeX project — powered by harbor-tex";

  inputs = {
    harbor.url = "git+https://github.com/caniko/harbor.git?ref=feat/harbor-monorepo-components&rev=7d99eb50c52d0a941e2996b97c469b32a7657ef4";
    nixpkgs.follows = "harbor/nixpkgs";
    nixpkgs-darwin.follows = "harbor/nixpkgs-darwin";
    treefmt-nix.follows = "harbor/treefmt-nix";
    git-hooks.follows = "harbor/git-hooks";
  };

  outputs = {
    self,
    nixpkgs,
    nixpkgs-darwin,
    harbor,
    treefmt-nix,
    git-hooks,
  }: let
    systems = [
      "x86_64-linux"
      "aarch64-linux"
      "x86_64-darwin"
      "aarch64-darwin"
    ];
    forSystem = system: let
      platformNixpkgs =
        if system == "x86_64-darwin"
        then nixpkgs-darwin
        else nixpkgs;
      pkgs = import platformNixpkgs {inherit system;};
      treefmtEval = treefmt-nix.lib.evalModule pkgs (import ./nix/treefmt.nix {inherit harbor;});
      hooks = import "${git-hooks}/nix" {
        nixpkgs = platformNixpkgs;
        inherit system;
        isFlakes = true;
      };
      pre-commit-check = hooks.run {
        src = ./.;
        hooks = import ./nix/pre-commit.nix {
          inherit pkgs;
          treefmtWrapper = treefmtEval.config.build.wrapper;
        };
      };
    in {
      inherit pkgs treefmtEval pre-commit-check;
      default = harbor.lib.tex.mkLatexDocument {
        inherit pkgs;
        name = "harbor-tex-template";
        src = ./.;
        mainFile = "main.tex";
        engine = "pdflatex";
        profile = "cv";
      };
      shell = harbor.lib.tex.mkTexDevShell {
        inherit pkgs;
        profile = "cv";
        extraPackages = pre-commit-check.enabledPackages;
        shellArgs.shellHook = pre-commit-check.shellHook;
      };
    };
  in {
    packages = nixpkgs.lib.genAttrs systems (system: {
      default = (forSystem system).default;
    });

    devShells = nixpkgs.lib.genAttrs systems (system: {
      default = (forSystem system).shell;
    });

    formatter = nixpkgs.lib.genAttrs systems (
      system: (forSystem system).treefmtEval.config.build.wrapper
    );

    checks = nixpkgs.lib.genAttrs systems (system: {
      formatting = (forSystem system).treefmtEval.config.build.check self;
    });
  };
}
