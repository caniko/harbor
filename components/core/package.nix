{pkgs}:
pkgs.rustPlatform.buildRustPackage {
  pname = "harbor-evidence";
  version = "0.1.0";
  src = pkgs.lib.cleanSource ../..;
  cargoLock = {
    lockFile = ../../Cargo.lock;
    # The shared workspace lock also contains CAD's pinned Fleetix dependency.
    # This binds its exact source bytes even though this package does not use it.
    outputHashes."fleetix-0.4.0" = "sha256-m/4aN6/6zS801q8Y1jww1k65z+3x3QlXiaPkyFCmacQ=";
  };
  cargoBuildFlags = ["-p" "harbor-evidence"];
  cargoTestFlags = ["-p" "harbor-evidence"];
  strictDeps = true;
  meta = {
    description = "Shared Harbor development and production artifact contracts";
    homepage = "https://github.com/caniko/harbor";
    license = with pkgs.lib.licenses; [mit asl20];
    mainProgram = "harbor";
  };
}
