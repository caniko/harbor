{
  description = "Rust project — powered by harbor-rs";

  inputs = {
    harbor.url = "git+https://github.com/caniko/harbor.git?ref=feat/harbor-monorepo-components&rev=7d99eb50c52d0a941e2996b97c469b32a7657ef4";
    nixpkgs.follows = "harbor/nixpkgs";
    rust-overlay.follows = "harbor/rust-overlay";
    crane.follows = "harbor/crane";
    treefmt-nix.follows = "harbor/treefmt-nix";
    git-hooks.follows = "harbor/git-hooks";
  };

  outputs = {
    self,
    nixpkgs,
    harbor,
    rust-overlay,
    treefmt-nix,
    git-hooks,
    ...
  }: let
    systems = ["x86_64-linux" "aarch64-linux"];
    forSystem = system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [(import rust-overlay)];
      };
      toolchain = harbor.lib.rust.mkToolchain {inherit pkgs;};
      inherit (toolchain) craneLib rustToolchain;
      cross = harbor.lib.rust.mkCross {inherit pkgs system;};
      fmtToolchain = harbor.lib.rust.mkToolchain {
        inherit pkgs;
        toolchainProfile = "nightly";
      };
      treefmtEval = treefmt-nix.lib.evalModule pkgs (import ./nix/treefmt.nix {
        inherit harbor;
        rustfmtPackage = fmtToolchain.rustToolchain;
      });
      pre-commit-check = git-hooks.lib.${system}.run {
        src = ./.;
        hooks = import ./nix/pre-commit.nix {
          inherit pkgs harbor;
          treefmtWrapper = treefmtEval.config.build.wrapper;
          inherit rustToolchain;
        };
      };
      package = craneLib.buildPackage {
        src = ./.;
        pname = "cross-fixture";
        version = "0.1.0";
      };
    in {
      inherit pkgs toolchain craneLib cross treefmtEval pre-commit-check package;
    };
  in {
    packages = nixpkgs.lib.genAttrs systems (system: {
      default = (forSystem system).package;
    });

    formatter = nixpkgs.lib.genAttrs systems (
      system: (forSystem system).treefmtEval.config.build.wrapper
    );

    checks = nixpkgs.lib.genAttrs systems (
      system: let
        cfg = forSystem system;
      in {
        default = cfg.package;
        formatting = cfg.treefmtEval.config.build.check self;
      }
    );

    devShells = nixpkgs.lib.genAttrs systems (
      system: let
        cfg = forSystem system;
      in
        harbor.lib.rust.mkDevShells {
          inherit (cfg) pkgs cross;
          inherit (cfg.toolchain) craneLib;
          packages = cfg.pre-commit-check.enabledPackages;
          extraShellHook = cfg.pre-commit-check.shellHook;
        }
    );
  };
}
