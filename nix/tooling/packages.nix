{
  self,
  pkgs,
  config,
}: let
  cfg = config.programs.harbor;
  system = pkgs.stdenv.hostPlatform.system;
  rustPkgs = pkgs.extend self.inputs.rust-overlay.overlays.default;
  rust = self.lib.rust.mkToolchain {
    pkgs = rustPkgs;
    toolchainProfile = cfg.settings.rust.profile;
    cache.enable = false;
  };
  android = assert pkgs.lib.assertMsg
  (cfg.settings.android.platformVersions != [] && cfg.settings.android.buildToolsVersions != [] && cfg.settings.android.ndkVersions != [])
  "harbor: configure settings.android platformVersions, buildToolsVersions and ndkVersions, or override tools.android.package";
    self.lib.android.mkAndroidSdk ({inherit pkgs;} // cfg.settings.android);
  ntt = assert pkgs.lib.assertMsg
  (cfg.settings.ntt.pinsFile != null && cfg.settings.ntt.rustToolchain != null)
  "harbor: configure settings.ntt.pinsFile and rustToolchain, or override tools.ntt.package";
    self.lib.blockchain.ntt.mkNtt {
      inherit pkgs;
      pins = builtins.fromJSON (builtins.readFile cfg.settings.ntt.pinsFile);
      anchorRustToolchain = cfg.settings.ntt.rustToolchain;
    };
in {
  harbor-opencode = self.lib.core.opencode.mkCli {
    inherit pkgs;
    profiles = pkgs.lib.optionalAttrs cfg.tools.rust.enable self.lib.rust.opencodeRust.profiles;
  };
  inherit (pkgs) nixd taplo treefmt sccache uv ruff pyright nodejs bun gradle texlab pandoc mdbook foundry solc;
  rust = rust.rustToolchain;
  harbor-rs = self.packages.${system}.rust-harbor-rs;
  harbor-ci = self.packages.${system}.rust-harbor-ci;
  python = pkgs.python313;
  node = pkgs.nodejs;
  pnpm =
    (self.lib.javascript.node.mkNodeToolchain {
      inherit pkgs;
      nodejs = cfg.tools.node.package;
      # The retained Intel package set predates pnpm's security fixes. Reuse
      # the shared pin's maintained builder with the consumer's native Node.
      pnpm = (pkgs.callPackage (self.inputs.nixpkgs + "/pkgs/development/tools/pnpm") {}).pnpm_10.override {
        nodejs-slim = cfg.tools.node.package;
      };
    }).pnpm;
  typescript = pkgs.typescript-language-server;
  go = self.packages.${system}.go-go;
  gopls = self.packages.${system}.go-gopls;
  golangci-lint = self.packages.${system}.go-golangci-lint;
  android = android.androidsdk;
  android-device = self.lib.android.mkAndroidDeviceTools {
    inherit pkgs;
    adbPackage =
      if cfg.tools.android.enable
      then cfg.tools.android.package
      else null;
  };
  cargo-ndk = pkgs.cargo-ndk;
  jdk = pkgs.jdk21;
  texlive = self.lib.tex.mkTexlive {
    inherit pkgs;
    profile = cfg.settings.tex.profile;
  };
  anx-pandoc = self.packages.${system}.tex-anx-plugin-pandoc;
  anx-zenodo = self.packages.${system}.tex-anx-plugin-zenodo;
  database = self.packages.${system}.database-harbor-db;
  anchor = self.packages.${system}.solana-anchor;
  solana = self.packages.${system}.solana-solana-cli;
  cargo-build-sbf = self.packages.${system}.solana-cargo-build-sbf;
  ntt = pkgs.buildEnv {
    name = "harbor-ntt-toolchain";
    paths = ntt.fragment.packages;
    pathsToLink = ["/bin"];
  };
  llm = self.packages.${system}.llm-default;
  cad = self.packages.${system}.cad-default;
  freecad = self.packages.${system}.cad-gui;
  blender = pkgs.blender;
  openpencil = self.packages.${system}.open-pencil-cli;
  macos-sdk = pkgs.callPackage ../../components/rust/nix/validate-macos-sdk.nix {};
}
