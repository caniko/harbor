{
  description = "Bevy game project — powered by harbor-rs";

  inputs = {
    harbor-rs.url = "git+https://github.com/caniko/harbor-rs.git?ref=trunk&rev=7a3328e186258dca31f9801227bc4e6fd8db4f36";

    nixpkgs.follows = "harbor-rs/nixpkgs";
    rust-overlay.follows = "harbor-rs/rust-overlay";
    crane.follows = "harbor-rs/crane";
    flake-utils.url = "github:numtide/flake-utils";

    # Uncomment to enable AppImage packaging (Linux only):
    # nix-appimage.url = "github:ralismark/nix-appimage";
  };

  outputs = {
    self,
    nixpkgs,
    harbor-rs,
    flake-utils,
    rust-overlay,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [(import rust-overlay)];
      };

      toolchain = harbor-rs.lib.mkToolchain {inherit pkgs;};
      cross = harbor-rs.lib.mkCross {inherit pkgs system;};
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
      inherit (toolchain) craneLib;

      build = import ./nix/package.nix {inherit craneLib bevyDeps src;};
    in {
      packages.default = build.default;

      # Uncomment for AppImage packaging (requires nix-appimage input above):
      # packages.appimage = harbor-rs.lib.mkAppImage {
      #   inherit system nix-appimage;
      #   program = "${build.default}/bin/my-bevy-game";
      # };

      # Uncomment for Flatpak manifest generation:
      # packages.flatpak-manifest = (harbor-rs.lib.mkFlatpakManifest {
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
        inherit pkgs harbor-rs toolchain cross cargoConfig bevyDeps;
        checks = self.checks.${system};
      };
    });
}
