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

    harbor-meta = {
      url = "git+https://github.com/caniko/harbor-meta.git?ref=feat/shared-timezone-env&rev=4b94a2a9b9d9640bc08171c9a40edad964595c86";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    harbor-android = {
      url = "github:caniko/harbor-android/trunk";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.harbor-meta.follows = "harbor-meta";
    };

    nix-opencode-lsp = {
      url = "git+https://github.com/caniko/nix-opencode-lsp.git?ref=trunk";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    nix-bundle = {
      url = "github:nix-community/nix-bundle";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    git-hooks = {
      url = "github:cachix/git-hooks.nix";
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
    harbor-meta,
    harbor-android,
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
          harbor-meta = harbor-meta.lib;
          nixBundle = nix-bundle;
          harbor-android = harbor-android.lib;
        };
      in {
        inherit lib;
        treefmtModules.rust = ./nix/treefmt/rust.nix;

        sccache = sccacheLib;

        nixosModules = {
          macosSdk = import ./nix/nixos-module.nix;
          sccache = import ./nix/sccache-module.nix;
          buildCache = import ./nix/build-cache-module.nix;
          mavenCache = import ./nix/maven-cache-module.nix;
        };

        homeManagerModules.sccache = import ./nix/sccache-home.nix;

        templates.default = {
          path = ./templates/default;
          description = "Rust project with harbor-rs";
        };

        templates.bevy = {
          path = ./templates/bevy;
          description = "Bevy game engine project with harbor-rs cross-compilation";
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
        inherit (toolchain) cargoConfig;
        treefmt = inputs.treefmt-nix.lib.evalModule pkgs (import ./nix/treefmt.nix {
          harbor-rs = self;
          rustfmtPackage = toolchain.rustToolchain;
        });
        rsHarborVersion = (builtins.fromTOML (builtins.readFile ./cli/Cargo.toml)).package.version;

        bootstrapCmdsMig = import ./nix/bootstrap-cmds-mig.nix {
          inherit pkgs;
        };

        rsHarborCli = import ./nix/harbor-rs-cli.nix {
          inherit pkgs;
          inherit (toolchain) craneLib;
          src = ./.;
          version = rsHarborVersion;
        };

        harborCi = import ./nix/harbor-ci.nix {
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
              pname = "harbor-rs";
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
              pname = "harbor-rs";
              version = rsHarborVersion;
              artifacts = {
                x86_64-linux-musl = {
                  package = rsHarborStaticPackages.harbor-rs-x86_64-linux-musl;
                  system = "x86_64-linux";
                  rustTarget = "x86_64-unknown-linux-musl";
                  binutils = pkgs.pkgsStatic.stdenv.cc.bintools;
                  strip = "${pkgs.pkgsStatic.stdenv.cc.bintools}/bin/x86_64-unknown-linux-musl-strip";
                  readelf = "${pkgs.pkgsStatic.stdenv.cc.bintools.bintools}/bin/x86_64-unknown-linux-musl-readelf";
                  binaries = ["harbor-rs"];
                };
                aarch64-linux-musl = {
                  package = rsHarborStaticPackages.harbor-rs-aarch64-linux-musl;
                  system = "aarch64-linux";
                  rustTarget = "aarch64-unknown-linux-musl";
                  binutils = pkgs.pkgsCross.aarch64-multiplatform-musl.stdenv.cc.bintools;
                  strip = "${pkgs.pkgsCross.aarch64-multiplatform-musl.stdenv.cc.bintools}/bin/aarch64-unknown-linux-musl-strip";
                  readelf = "${pkgs.pkgsCross.aarch64-multiplatform-musl.stdenv.cc.bintools.bintools}/bin/aarch64-unknown-linux-musl-readelf";
                  binaries = ["harbor-rs"];
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

        # Language-aware `harbor-opencode`: the harbor-meta engine bound to
        # the rust profile registry. The harbor-meta package itself stays
        # profile-less (policy-only).
        harborOpencode = self.lib.opencodeRust.mkCli {inherit pkgs;};
      in {
        packages =
          {
            # Consumers use this build-platform package to keep the
            # compiler-cache executable and version identical across the
            # Atlas fleet and every harbor-rs builder.
            inherit (pkgs) sccache;
            sccache-user-daemon-client = sccacheLib.mkClientWrapper {
              inherit pkgs;
              sccachePackage = pkgs.sccache;
            };
            validate-macos-sdk = validateMacosSdk;
            stage-macos-universal = macosStaging.stager;
            bootstrap-cmds-mig = bootstrapCmdsMig;
            steam-runtime-cargo-bootstrap = steamRuntimeTools.steamRuntimeCargoBootstrap;
            harbor-rs = rsHarborCli;
            rs-harbor = rsHarborCli;
            harbor-ci = harborCi;
            harbor-opencode = harborOpencode;
          }
          // (
            if rsHarborBinaryRelease != null
            then {
              inherit (rsHarborStaticPackages) harbor-rs-x86_64-linux-musl;
              inherit (rsHarborStaticPackages) harbor-rs-aarch64-linux-musl;
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
          harbor-opencode = {
            type = "app";
            program = "${harborOpencode}/bin/harbor-opencode";
          };
        };

        devShells = import ./nix/dev-shells.nix {
          harbor = self.lib;
          opencodeLsp = nix-opencode-lsp.lib;
          inherit pkgs toolchain cross cargoConfig rsHarborCli harborCi;
          treefmtWrapper = treefmt.config.build.wrapper;
        };

        formatter = treefmt.config.build.wrapper;

        checks =
          (import ./checks.nix {
            inherit self pkgs system toolchain cross nixpkgs rust-overlay crane;
            inherit (inputs) treefmt-nix git-hooks;
            rootInputNames = builtins.attrNames inputs;
          })
          // (import ./nix/checks-dev-sandbox.nix {
            inherit pkgs;
            harborRsCli = rsHarborCli;
            inherit (self.lib) mkDevSandbox;
          })
          // {
            treefmt-modules = import ./nix/treefmt/check.nix {
              inherit pkgs;
              inherit (inputs) treefmt-nix;
              inherit (toolchain) rustToolchain;
              rustModule = self.treefmtModules.rust;
              baseModules = [harbor-meta.treefmtModules.nix harbor-meta.treefmtModules.toml];
            };
          };
      };
    };
}
