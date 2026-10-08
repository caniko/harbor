{
  harbor-meta,
  harbor-rs,
  solanaSource,
  solanaSourceDarwin,
}: rec {
  timezone = harbor-meta.lib.timezone;
  rustOverlay = import harbor-rs.inputs.rust-overlay;

  mkCargoBuildSbf = {
    pkgs,
    solana ? pkgs.solana-cli,
    source ?
      if pkgs.stdenv.hostPlatform.system == "x86_64-darwin"
      then solanaSourceDarwin
      else solanaSource,
    src ?
      if builtins.pathExists (source + "/platform-tools-sdk/Cargo.toml")
      then source + "/platform-tools-sdk"
      else source,
  }: let
    toolchain = harbor-rs.lib.mkToolchain {inherit pkgs;};
    manifest = builtins.fromTOML (builtins.readFile (src + "/Cargo.toml"));
    commonArgs = {
      inherit src;
      cargoLock = src + "/Cargo.lock";
      pname = "cargo-build-sbf";
      version = assert manifest.workspace.package.version == solana.version; solana.version;
      cargoExtraArgs = "-p solana-cargo-build-sbf";
      rsHarborCargoTomlContents = builtins.readFile (src + "/Cargo.toml");
      doCheck = false;
      strictDeps = true;
      nativeBuildInputs = [pkgs.pkg-config];
      buildInputs = [pkgs.bzip2 pkgs.openssl];
    };
    # Agave 3 patches registry dependencies with real workspace crates; Crane's
    # dependency-only dummy sources cannot satisfy those path patches.
    cargoArtifacts =
      if builtins.pathExists (source + "/platform-tools-sdk/Cargo.toml")
      then toolchain.craneLib.buildDepsOnly commonArgs
      else null;
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

  mkSolanaDevShell = {
    pkgs,
    timeZone ? "UTC",
    ...
  } @ args:
    harbor-meta.lib.devShell.mkShell {
      inherit pkgs timeZone;
      fragments = [
        (mkSolanaDevShellFragment (builtins.removeAttrs args ["timeZone"]))
      ];
    };
}
