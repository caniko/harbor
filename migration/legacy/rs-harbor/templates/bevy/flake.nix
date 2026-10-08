{
  description = "Bevy game project — powered by rs-harbor";

  inputs = {
    rs-harbor.url = "git+ssh://git@codeberg.org/caniko/rs-harbor.git?ref=trunk&rev=f209ddbca3fdbb0dc31fa3886ccc2ff7369c18ac";

    nixpkgs.follows = "rs-harbor/nixpkgs";
    rust-overlay.follows = "rs-harbor/rust-overlay";
    crane.follows = "rs-harbor/crane";
    flake-utils.url = "github:numtide/flake-utils";

    # Uncomment to enable AppImage packaging (Linux only):
    # nix-appimage.url = "github:ralismark/nix-appimage";
  };

  outputs = {
    self,
    nixpkgs,
    rs-harbor,
    flake-utils,
    rust-overlay,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [(import rust-overlay)];
      };

      toolchain = rs-harbor.lib.mkToolchain {inherit pkgs;};
      cross = rs-harbor.lib.mkCross {inherit pkgs system;};
      cargoConfig = rs-harbor.lib.mkCargoConfig {
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
      inherit (toolchain) craneLib;

      build = import ./nix/package.nix {inherit craneLib bevyDeps src;};
    in {
      packages.default = build.default;

      # Uncomment for AppImage packaging (requires nix-appimage input above):
      # packages.appimage = rs-harbor.lib.mkAppImage {
      #   inherit system nix-appimage;
      #   program = "${build.default}/bin/my-bevy-game";
      # };

      # Uncomment for Flatpak manifest generation:
      # packages.flatpak-manifest = (rs-harbor.lib.mkFlatpakManifest {
      #   inherit pkgs;
      #   appId = "com.example.MyBevyGame";
      #   pname = "my-bevy-game";
      #   desktopFile = ''
      #     [Desktop Entry]
      #     Type=Application
      #     Name=My Bevy Game
      #     Exec=my-bevy-game
      #     Icon=com.example.MyBevyGame
      #     Categories=Game;
      #   '';
      #   finishArgs = [
      #     "--share=ipc"
      #     "--socket=x11"
      #     "--socket=wayland"
      #     "--device=dri"
      #     "--socket=pulseaudio"
      #   ];
      # }).manifestPath;

      checks = {
        inherit (build) default clippy fmt;
      };

      devShells = import ./nix/dev-shells.nix {
        inherit pkgs rs-harbor toolchain cross cargoConfig bevyDeps;
        checks = self.checks.${system};
      };
    });
}
