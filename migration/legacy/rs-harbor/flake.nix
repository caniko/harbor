{
  description = "Reusable Rust cross-compilation and toolchain infrastructure for Nix flakes";

  inputs = {
    crane.url = "github:ipetkov/crane";

    flake-parts.url = "github:hercules-ci/flake-parts";

    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    osxcross = {
      url = "github:caniko/osxcross/flake";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    meta-harbor = {
      url = "git+ssh://git@codeberg.org/caniko/meta-harbor.git?ref=trunk";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    nix-opencode-lsp = {
      url = "git+ssh://git@github.com/caniko/nix-opencode-lsp.git?ref=trunk";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    nix-bundle = {
      url = "github:nix-community/nix-bundle";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs @ {
    flake-parts,
    self,
    nixpkgs,
    crane,
    rust-overlay,
    osxcross,
    meta-harbor,
    nix-opencode-lsp,
    nix-bundle,
    ...
  }: let
    sccacheLib = import ./sccache/lib {
      inherit nixpkgs;
      sccacheDefault = import ./lib/generated/sccache-default.nix;
    };
  in
    flake-parts.lib.mkFlake {inherit inputs;} {
      systems = ["x86_64-linux" "aarch64-linux"];

      flake = let
        lib = import ./lib {
          inherit crane osxcross;
          meta-harbor = meta-harbor.lib;
          nixBundle = nix-bundle;
        };
      in {
        inherit lib;

        sccache = sccacheLib;

        nixosModules = {
          macosSdk = import ./nix/nixos-module.nix;
          sccache = import ./nix/sccache-module.nix;
          buildCache = import ./nix/build-cache-module.nix;
        };

        homeManagerModules.sccache = import ./nix/sccache-home.nix;

        templates.bevy = {
          path = ./templates/bevy;
          description = "Bevy game engine project with rs-harbor cross-compilation";
        };
      };

      perSystem = {system, ...}: let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [(import rust-overlay)];
        };

        validateMacosSdk = import ./nix/validate-macos-sdk.nix {
          inherit pkgs;
        };

        toolchain = self.lib.mkToolchain {
          inherit pkgs;
          toolchainProfile = "nightly";
        };
        cross = self.lib.mkCross {inherit pkgs system;};
        cargoConfig = toolchain.cargoConfig;
        rsHarborVersion = (builtins.fromTOML (builtins.readFile ./cli/Cargo.toml)).package.version;

        bootstrapCmdsMig = import ./nix/bootstrap-cmds-mig.nix {
          inherit pkgs;
        };

        rsHarborCli = import ./nix/rs-harbor-cli.nix {
          inherit pkgs;
          inherit (toolchain) craneLib;
          src = ./.;
          version = rsHarborVersion;
        };

        rsHarborStaticPackages =
          if system == "x86_64-linux"
          then
            self.lib.mkCrossPackages {
              inherit pkgs cross;
              inherit (toolchain) craneLib;
              pname = "rs-harbor";
              commonArgs = {
                src = ./.;
                version = rsHarborVersion;
                strictDeps = true;
                doCheck = false;
                nativeBuildInputs = [pkgs.clang pkgs.mold];
                nativeCheckInputs = [pkgs.git];
              };
              targets = ["x86_64-linux-musl" "aarch64-linux-musl"];
            }
          else {};

        rsHarborBinaryRelease =
          if system == "x86_64-linux"
          then
            self.lib.mkBinaryRelease {
              inherit pkgs;
              pname = "rs-harbor";
              version = rsHarborVersion;
              artifacts = {
                x86_64-linux-musl = {
                  package = rsHarborStaticPackages.rs-harbor-x86_64-linux-musl;
                  system = "x86_64-linux";
                  rustTarget = "x86_64-unknown-linux-musl";
                  binutils = pkgs.pkgsStatic.stdenv.cc.bintools;
                  strip = "${pkgs.pkgsStatic.stdenv.cc.bintools}/bin/x86_64-unknown-linux-musl-strip";
                  readelf = "${pkgs.pkgsStatic.stdenv.cc.bintools.bintools}/bin/x86_64-unknown-linux-musl-readelf";
                  binaries = ["rs-harbor"];
                };
                aarch64-linux-musl = {
                  package = rsHarborStaticPackages.rs-harbor-aarch64-linux-musl;
                  system = "aarch64-linux";
                  rustTarget = "aarch64-unknown-linux-musl";
                  binutils = pkgs.pkgsCross.aarch64-multiplatform-musl.stdenv.cc.bintools;
                  strip = "${pkgs.pkgsCross.aarch64-multiplatform-musl.stdenv.cc.bintools}/bin/aarch64-unknown-linux-musl-strip";
                  readelf = "${pkgs.pkgsCross.aarch64-multiplatform-musl.stdenv.cc.bintools.bintools}/bin/aarch64-unknown-linux-musl-readelf";
                  binaries = ["rs-harbor"];
                };
              };
            }
          else null;

        rsHarborReleaseBundle =
          if rsHarborBinaryRelease != null
          then rsHarborBinaryRelease.releaseBundle
          else null;

        macosStaging = self.lib.mkMacosUniversalStager {
          inherit pkgs rsHarborCli;
        };

        macosSdkTools = import ./nix/macos-sdk-tools.nix {
          inherit pkgs rsHarborCli validateMacosSdk;
          realizeMacosSdkBin = osxcross.packages.${system}.realize-macos-sdk;
        };

        steamRuntimeTools = self.lib.mkSteamRuntimeTools {
          inherit pkgs rsHarborCli;
        };
      in {
        packages =
          {
            # Consumers use this build-platform package to keep the
            # compiler-cache executable and version identical across the
            # Atlas fleet and every rs-harbor builder.
            sccache = pkgs.sccache;
            sccache-user-daemon-client = sccacheLib.mkClientWrapper {
              inherit pkgs;
              sccachePackage = pkgs.sccache;
            };
            validate-macos-sdk = validateMacosSdk;
            stage-macos-universal = macosStaging.stager;
            bootstrap-cmds-mig = bootstrapCmdsMig;
            steam-runtime-cargo-bootstrap = steamRuntimeTools.steamRuntimeCargoBootstrap;
            rs-harbor = rsHarborCli;
          }
          // (
            if rsHarborBinaryRelease != null
            then {
              rs-harbor-x86_64-linux-musl = rsHarborStaticPackages.rs-harbor-x86_64-linux-musl;
              rs-harbor-aarch64-linux-musl = rsHarborStaticPackages.rs-harbor-aarch64-linux-musl;
              release-bundle = rsHarborReleaseBundle;
            }
            else {}
          );

        apps = {
          publish-macos-sdk = {
            type = "app";
            program = "${macosSdkTools.publishMacosSdk}/bin/publish-macos-sdk";
          };
          realize-macos-sdk = {
            type = "app";
            program = "${macosSdkTools.realizeMacosSdkBin}/bin/realize-macos-sdk";
          };
          validate-macos-sdk = {
            type = "app";
            program = "${validateMacosSdk}/bin/validate-macos-sdk";
          };
          stage-macos-universal = {
            type = "app";
            program = "${macosStaging.stager}/bin/stage-macos-universal";
          };
          generate-jetbrains-signing-material = {
            type = "app";
            program = "${self.lib.mkJetBrainsSigningMaterial {inherit pkgs;}}/bin/generate-jetbrains-signing-material";
          };
        };

        devShells = import ./nix/dev-shells.nix {
          harbor = self.lib;
          opencodeLsp = nix-opencode-lsp.lib;
          inherit pkgs toolchain cross cargoConfig rsHarborCli;
        };

        checks = import ./checks.nix {
          inherit self pkgs system toolchain cross;
          rootInputNames = builtins.attrNames inputs;
        };
      };
    };
}
