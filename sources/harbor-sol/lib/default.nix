{
  harbor-meta,
  harbor-rs,
}: rec {
  rustOverlay = import harbor-rs.inputs.rust-overlay;

  mkCargoBuildSbf = {
    pkgs,
    solana ? pkgs.solana-cli,
  }: let
    toolchain = harbor-rs.lib.mkToolchain {inherit pkgs;};
    src = solana.src + "/platform-tools-sdk";
    commonArgs = {
      inherit src;
      cargoLock = src + "/Cargo.lock";
      pname = "cargo-build-sbf";
      version = solana.version;
      cargoExtraArgs = "-p solana-cargo-build-sbf";
      rsHarborCargoTomlContents = builtins.readFile (src + "/Cargo.toml");
      doCheck = false;
      strictDeps = true;
      nativeBuildInputs = [pkgs.pkg-config];
      buildInputs = [pkgs.bzip2 pkgs.openssl];
    };
    cargoArtifacts = toolchain.craneLib.buildDepsOnly commonArgs;
  in
    toolchain.craneLib.buildPackage (commonArgs // {inherit cargoArtifacts;});

  mkSolanaToolchain = {
    pkgs,
    anchor ? pkgs.anchor,
    solana ? pkgs.solana-cli,
    cargoBuildSbf ? mkCargoBuildSbf {inherit pkgs solana;},
    rustToolchain ? (harbor-rs.lib.mkToolchain {inherit pkgs;}).rustToolchain,
  }: let
    rustupToolchain = "harbor-sol-${builtins.substring 0 12 (builtins.hashString "sha256" (toString rustToolchain))}";
  in {
    packages = [pkgs.rustup anchor cargoBuildSbf solana rustToolchain];
    env = {
      ANCHOR_VERSION = anchor.version;
      SOLANA_VERSION = solana.version;
    };
    shellHook = ''
      export PATH="${pkgs.rustup}/bin:$PATH"
      export RUSTUP_HOME="''${XDG_CACHE_HOME:-$HOME/.cache}/harbor-sol/rustup"
      mkdir -p "$RUSTUP_HOME"
      if ! ${pkgs.rustup}/bin/rustup toolchain list | grep -F ${rustupToolchain} >/dev/null; then
        ${pkgs.rustup}/bin/rustup toolchain link ${rustupToolchain} ${rustToolchain}
      fi
      ${pkgs.rustup}/bin/rustup default ${rustupToolchain} >/dev/null
    '';
  };

  mkSolanaDevShellFragment = args: let
    toolchain = mkSolanaToolchain args;
  in {
    inherit (toolchain) packages env shellHook;
  };

  mkSolanaDevShell = {pkgs, ...} @ args:
    harbor-meta.lib.devShell.mkShell {
      inherit pkgs;
      fragments = [
        (mkSolanaDevShellFragment args)
      ];
    };
}
