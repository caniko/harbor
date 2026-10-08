{
  pkgs,
  craneLib,
  src ? ../.,
  version ? "0.1.0",
}: let
  source = pkgs.lib.cleanSourceWith {
    inherit src;
    filter = path: type:
      (!pkgs.lib.hasPrefix (toString (src + "/.cargo")) (toString path))
      && (
        craneLib.filterCargoSources path type
        || (builtins.match ".*/tests/fixtures/.*" path != null)
      );
    name = "harbor-ci-source";
  };
in
  craneLib.buildPackage {
    pname = "harbor-ci";
    inherit version;
    src = source;
    strictDeps = true;
    doCheck = true;
    cargoBuildCommand = "cargo build --release -p harbor-xtask --bin harbor-ci";
    cargoInstallCommand = "install -Dm755 target/release/harbor-ci $out/bin/harbor-ci";
    nativeBuildInputs = [
      pkgs.clang
      pkgs.mold
    ];
    nativeCheckInputs = [pkgs.git];
  }
