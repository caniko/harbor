{pkgs}:
pkgs.rustPlatform.buildRustPackage {
  pname = "harbor-evidence";
  version = "0.1.0";
  src = pkgs.lib.cleanSource ../..;
  cargoLock.lockFile = ../../Cargo.lock;
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
