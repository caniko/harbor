{
  description = "Bevy game project — powered by harbor-rs";

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
      cargoConfig = harbor-rs.lib.mkCargoConfig {
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
          inherit harbor-rs;
          extraPackages = cfg.pre-commit-check.enabledPackages;
          extraShellHook = cfg.pre-commit-check.shellHook;
        }
    );
  };
}
