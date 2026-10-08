{
  description = "Rust project — powered by harbor-rs";

  inputs = {
    harbor-rs.url = "git+https://github.com/caniko/harbor-rs.git?ref=trunk";
    nixpkgs.follows = "harbor-rs/nixpkgs";
    rust-overlay.follows = "harbor-rs/rust-overlay";
    crane.follows = "harbor-rs/crane";
    treefmt-nix.follows = "harbor-rs/treefmt-nix";
    git-hooks.follows = "harbor-rs/git-hooks";
  };

  outputs = {
    self,
    nixpkgs,
    harbor-rs,
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
      toolchain = harbor-rs.lib.mkToolchain {inherit pkgs;};
      inherit (toolchain) craneLib rustToolchain;
      cross = harbor-rs.lib.mkCross {inherit pkgs system;};
      fmtToolchain = harbor-rs.lib.mkToolchain {
        inherit pkgs;
        toolchainProfile = "nightly";
      };
      treefmtEval = treefmt-nix.lib.evalModule pkgs (import ./nix/treefmt.nix {
        inherit harbor-rs;
        rustfmtPackage = fmtToolchain.rustToolchain;
      });
      pre-commit-check = git-hooks.lib.${system}.run {
        src = ./.;
        hooks = import ./nix/pre-commit.nix {
          inherit pkgs harbor-rs;
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
        harbor-rs.lib.mkDevShells {
          inherit (cfg) pkgs cross;
          inherit (cfg.toolchain) craneLib;
          packages = cfg.pre-commit-check.enabledPackages;
          extraShellHook = cfg.pre-commit-check.shellHook;
        }
    );
  };
}
