{
  description = "harbor-db - secure generic lifecycle plans and NixOS systemd wiring";

  inputs = {
    harbor-rs.url = "git+https://github.com/caniko/harbor-rs.git?ref=feat/portable-release-files&rev=195f8a7e81d67102868631015b6794280b492c88";
    rs-harbor.follows = "harbor-rs";
    harbor-meta = {
      url = "git+https://github.com/caniko/harbor-meta.git?ref=feat/shared-timezone-env&rev=1f272a44dea9dc531b30efb384720ddb46f083e3";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane.url = "github:ipetkov/crane";
  };

  outputs = {
    self,
    harbor-rs,
    harbor-meta,
    nixpkgs,
    crane,
    ...
  }: let
    systems = [
      "x86_64-linux"
      "aarch64-linux"
    ];
    forAllSystems = f:
      nixpkgs.lib.genAttrs systems (system: let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [(import harbor-rs.inputs.rust-overlay)];
        };
        toolchain = harbor-rs.lib.mkToolchain {
          inherit pkgs;
          toolchainProfile = "stable";
        };
      in
        f {
          inherit system pkgs toolchain;
          craneLib = toolchain.craneLib;
        });
  in {
    lib.timezone = harbor-meta.lib.timezone;
    nixosModules.harbor-db = {
      lib,
      pkgs,
      ...
    }: {
      imports = [
        (import ./nix/module.nix)
        (lib.mkAliasOptionModule ["services" "db-harbor"] ["services" "harbor-db"])
      ];
      services.harbor-db.package = lib.mkDefault self.packages.${pkgs.system}.harbor-db;
    };
    nixosModules.pg-backup = import ./nix/pg-backup.nix;
    nixosModules.db-harbor = self.nixosModules.harbor-db;
    nixosModules.default = self.nixosModules.harbor-db;

    packages = forAllSystems ({
      pkgs,
      craneLib,
      ...
    }: let
      commonArgs = {
        src = craneLib.cleanCargoSource ./.;
        pname = "harbor-db";
        version = "0.1.0";
        strictDeps = true;
        cargoExtraArgs = "--locked";
        meta = {
          description = "Secure generic lifecycle plans and deployment orchestration for services";
          homepage = "https://github.com/caniko/harbor-db";
          license = pkgs.lib.licenses.asl20;
          mainProgram = "harbor-db";
        };
      };
      cargoArtifacts = craneLib.buildDepsOnly commonArgs;
      buildCache = harbor-rs.lib.mkBuildCachePolicy {
        inherit pkgs;
        sccachePackage = harbor-rs.packages.${pkgs.stdenv.hostPlatform.system}.sccache;
        cacheRoot = null;
        ephemeralFallback = true;
        namespaceScope = "canix-rust";
        namespaceGeneration = 5;
      };
      harbor-db = buildCache.withRustCache {
        package = craneLib.buildPackage (commonArgs // {inherit cargoArtifacts;});
      };
    in {
      inherit harbor-db;
      db-harbor = harbor-db;
      # Same derivation: exports both harbor-db and the standalone
      # home-manager-backup bin. mainProgram lets `lib.getExe` resolve the
      # backup helper on both supported architectures.
      home-manager-backup =
        harbor-db
        // {
          meta = harbor-db.meta // {mainProgram = "home-manager-backup";};
        };
      default = harbor-db;
    });

    checks = forAllSystems ({
      pkgs,
      craneLib,
      ...
    }: let
      src = craneLib.cleanCargoSource ./.;
      commonArgs = {
        inherit src;
        pname = "harbor-db";
        version = "0.1.0";
        strictDeps = true;
        cargoExtraArgs = "--locked";
      };
      cargoArtifacts = craneLib.buildDepsOnly commonArgs;
    in {
      module-eval = pkgs.callPackage ./nix/module-eval.nix {
        module = import ./nix/module.nix;
      };
      module-smoke = pkgs.callPackage ./nix/test-module.nix {
        module = self.nixosModules.default;
      };
      pg-backup-eval = pkgs.callPackage ./nix/pg-backup-eval.nix {};
      harbor-db = self.packages.${pkgs.stdenv.hostPlatform.system}.harbor-db;
      cargo-fmt = craneLib.cargoFmt {
        inherit src;
        pname = "harbor-db";
      };
      cargo-test = craneLib.cargoTest (commonArgs
        // {
          inherit cargoArtifacts;
          cargoExtraArgs = "--all-targets --all-features --locked";
        });
      cargo-clippy = craneLib.cargoClippy (commonArgs
        // {
          inherit cargoArtifacts;
          cargoExtraArgs = "--all-targets --all-features --locked";
          cargoClippyExtraArgs = "-- -D warnings";
        });
    });

    formatter = forAllSystems ({pkgs, ...}: pkgs.alejandra);

    devShells = forAllSystems ({pkgs, ...}: let
      packages = [
        pkgs.alejandra
        pkgs.cargo
        pkgs.cargo-nextest
        pkgs.clippy
        pkgs.gcc
        pkgs.nixd
        pkgs.rustc
        pkgs.rustfmt
      ];
    in {
      default = harbor-meta.lib.devShell.mkShell {inherit pkgs packages;};
      docs = harbor-meta.lib.devShell.mkShell {inherit pkgs packages;};
    });
  };
}
