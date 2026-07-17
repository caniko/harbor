{
  description = "Plugins for the anx article toolchain";

  inputs = {
    rs-harbor.url = "git+https://codeberg.org/caniko/rs-harbor.git?ref=trunk&rev=9bfa8bdb0ecb22d7bc11448665f7fbaebae7a759";
    nixpkgs.follows = "rs-harbor/nixpkgs";
    rust-overlay.follows = "rs-harbor/rust-overlay";
    crane.follows = "rs-harbor/crane";
  };

  outputs = {
    self,
    rs-harbor,
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
      toolchain = rs-harbor.lib.mkToolchain {inherit pkgs;};
      inherit (toolchain) craneLib rustToolchain;
      buildCache = rs-harbor.lib.mkBuildCachePolicy {
        inherit pkgs;
        buildPackageSet = pkgs.buildPackages;
        sccachePackage = pkgs.buildPackages.sccache;
        cacheRoot = null;
        namespaceScope = "canix-rust";
        namespaceGeneration = 5;
      };

      commonArgs = {
        src = craneLib.cleanCargoSource ./.;
        strictDeps = true;
        buildInputs = [];
        nativeBuildInputs = [];
      };

      cargoArtifacts = craneLib.buildDepsOnly commonArgs;

      anx-plugin-zenodo = buildCache.withRustCache {
        package = craneLib.buildPackage (commonArgs
          // {
            inherit cargoArtifacts;
            pname = "anx-plugin-zenodo";
          });
      };

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
