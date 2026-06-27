{
  description = "Plugins for the anx article toolchain";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    crane,
    rust-overlay,
    ...
  }: let
    systems = ["x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin"];

    forAllSystems = f:
      nixpkgs.lib.genAttrs systems (system:
        f (import nixpkgs {
          inherit system;
          overlays = [rust-overlay.overlays.default];
        }));
  in {
    packages = forAllSystems (pkgs: let
      rustToolchain = pkgs.rust-bin.stable.latest.default.override {
        extensions = ["rust-src" "rustfmt" "clippy"];
      };
      craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

      commonArgs = {
        src = craneLib.cleanCargoSource ./.;
        strictDeps = true;
        buildInputs = [];
        nativeBuildInputs = [];
      };

      cargoArtifacts = craneLib.buildDepsOnly commonArgs;

      anx-plugin-zenodo = craneLib.buildPackage (commonArgs
        // {
          inherit cargoArtifacts;
          pname = "anx-plugin-zenodo";
        });

      anx-plugin-pandoc = pkgs.python314.pkgs.buildPythonPackage {
        pname = "anx-plugin-pandoc";
        version = "0.1.0";
        pyproject = true;
        src = ./pandoc;
        nativeBuildInputs = [pkgs.python314.pkgs.hatchling];
        meta = {
          description = "Pandoc ODT export plugin for anx";
          license = nixpkgs.lib.licenses.asl20;
        };
      };
    in {
      inherit anx-plugin-zenodo anx-plugin-pandoc;
      default = pkgs.symlinkJoin {
        name = "anx-plugins";
        paths = [anx-plugin-zenodo anx-plugin-pandoc];
      };
    });

    devShells = forAllSystems (pkgs: let
      anx-plugin-zenodo = self.packages.${pkgs.system}.anx-plugin-zenodo;
    in {
      default = pkgs.mkShell {
        packages = [
          pkgs.cargo
          pkgs.rust-bin.stable.latest.default
          pkgs.python314
          pkgs.python314.pkgs.uv
          pkgs.pandoc
        ];
        inputsFrom = [anx-plugin-zenodo];
      };
    });

    checks = forAllSystems (pkgs: let
      rustToolchain = pkgs.rust-bin.stable.latest.default;
      craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
      commonArgs = {
        src = craneLib.cleanCargoSource ./.;
        strictDeps = true;
      };
      cargoArtifacts = craneLib.buildDepsOnly commonArgs;
    in {
      rust-fmt = craneLib.cargoFmt {
        src = craneLib.cleanCargoSource ./.;
      };
      rust-clippy = craneLib.cargoClippy (commonArgs // {
        inherit cargoArtifacts;
        cargoClippyExtraArgs = "-- --deny warnings";
      });
      pandoc-plugin-import = pkgs.runCommand "pandoc-plugin-test" {
        buildInputs = [self.packages.${pkgs.system}.anx-plugin-pandoc];
      } ''
        python -c "from anx_plugin_pandoc import export_odt; print('import OK')"
        touch $out
      '';
    });
  };
}
