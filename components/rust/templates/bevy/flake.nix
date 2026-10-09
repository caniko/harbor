{
  description = "Bevy game project — powered by harbor-rs";

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
      cargoConfig = harbor.lib.rust.mkCargoConfig {
        inherit pkgs;
        extraConfig = ''
          [alias]
          rd = "run --features bevy/dynamic_linking"
        '';
      };
      bevyDeps = import ./nix/bevy-deps.nix {inherit pkgs;};
      src = pkgs.lib.cleanSourceWith {
        src = ./.;
        filter = path: type:
          (!pkgs.lib.hasPrefix (toString ./.cargo) (toString path))
          && craneLib.filterCargoSources path type;
      };
      build = import ./nix/package.nix {inherit craneLib bevyDeps src;};
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
    in {
      inherit pkgs toolchain craneLib cross cargoConfig bevyDeps build treefmtEval pre-commit-check;
    };
  in {
    packages = nixpkgs.lib.genAttrs systems (system: {
      default = (forSystem system).build.default;
    });

    formatter = nixpkgs.lib.genAttrs systems (
      system: (forSystem system).treefmtEval.config.build.wrapper
    );

    checks = nixpkgs.lib.genAttrs systems (
      system: let
        cfg = forSystem system;
        inherit (cfg.build) default clippy fmt;
      in {
        inherit default clippy fmt;
        formatting = cfg.treefmtEval.config.build.check self;
      }
    );

    devShells = nixpkgs.lib.genAttrs systems (
      system: let
        cfg = forSystem system;
      in
        import ./nix/dev-shells.nix {
          inherit (cfg) pkgs toolchain cross cargoConfig bevyDeps;
          inherit harbor;
          extraPackages = cfg.pre-commit-check.enabledPackages;
          extraShellHook = cfg.pre-commit-check.shellHook;
        }
    );
  };
}
