{
  description = "LaTeX project — powered by harbor-tex";

  inputs = {
    harbor-tex.url = "github:caniko/harbor-tex";
    nixpkgs.follows = "harbor-tex/nixpkgs";
    nixpkgs-darwin.follows = "harbor-tex/nixpkgs-darwin";
    treefmt-nix.follows = "harbor-tex/treefmt-nix";
    git-hooks.follows = "harbor-tex/git-hooks";
  };

  outputs = {
    self,
    nixpkgs,
    nixpkgs-darwin,
    harbor-tex,
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
      platformNixpkgs = if system == "x86_64-darwin" then nixpkgs-darwin else nixpkgs;
      pkgs = import platformNixpkgs {inherit system;};
      treefmtEval = treefmt-nix.lib.evalModule pkgs (import ./nix/treefmt.nix);
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
      default = harbor-tex.lib.mkLatexDocument {
        inherit pkgs;
        name = "harbor-tex-template";
        src = ./.;
        mainFile = "main.tex";
        engine = "pdflatex";
        profile = "cv";
      };
      shell = harbor-tex.lib.mkTexDevShell {
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
