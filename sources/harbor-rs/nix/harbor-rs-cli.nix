{
  pkgs,
  craneLib,
  src ? ../.,
  version ? "0.1.0",
}: let
  rsHarborSrc = pkgs.lib.cleanSourceWith {
    inherit src;
    filter = path: type:
      (!pkgs.lib.hasPrefix (toString (src + "/.cargo")) (toString path))
      && (
        (craneLib.filterCargoSources path type)
        || (builtins.match ".*/tests/fixtures/.*" path != null)
      );
    name = "harbor-rs-source";
  };
in
  craneLib.buildPackage {
    pname = "harbor-rs";
    inherit version;
    src = rsHarborSrc;
    cargoExtraArgs = "-p nix_harbor_rs";
    strictDeps = true;
    doCheck = true;
    nativeBuildInputs = [
      pkgs.clang
      pkgs.mold
    ];
    nativeCheckInputs = [
      pkgs.git
    ];
  }
