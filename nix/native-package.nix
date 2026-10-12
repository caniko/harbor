{pkgs}: let
  manifest = builtins.fromTOML (builtins.readFile ../Cargo.toml);
in
  pkgs.rustPlatform.buildRustPackage {
    pname = "harbor-db-storage";
    inherit (manifest.package) version;
    src = pkgs.lib.cleanSource ../.;
    cargoLock.lockFile = ../Cargo.lock;
    cargoBuildFlags = ["--bins"];
    CARGO_BUILD_TARGET = pkgs.stdenv.hostPlatform.rust.rustcTarget;
    RUSTFLAGS = "-C target-feature=+crt-static -L native=${pkgs.glibc.static}/lib";
    doCheck = false;
    disallowedReferences = [pkgs.python3];
    passthru.harborDbRuntime = "rust";
    meta = {
      description = "Native crash-safe storage lifecycle and fenced recovery";
      homepage = "https://github.com/caniko/harbor-db";
      license = pkgs.lib.licenses.asl20;
      mainProgram = "harbor-db-postgres";
      platforms = pkgs.lib.platforms.linux;
    };
  }
