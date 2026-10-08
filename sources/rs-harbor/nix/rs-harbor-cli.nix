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
    name = "rs-harbor-source";
  };
in
  craneLib.buildPackage {
    pname = "rs-harbor";
    inherit version;
    src = rsHarborSrc;
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
