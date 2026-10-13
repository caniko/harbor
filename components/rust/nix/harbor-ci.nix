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
        # Workspace crates embed their README as compile-time rustdoc.
        || pkgs.lib.hasSuffix "/README.md" (toString path)
        || (builtins.match ".*/tests/fixtures/.*" path != null)
      );
    name = "harbor-ci-source";
  };
in
  craneLib.buildPackage {
    pname = "harbor-ci";
    inherit version;
    src = source;
    cargoExtraArgs = "-p harbor-xtask";
    strictDeps = true;
    doCheck = true;
    cargoBuildCommand = "cargo build --release -p harbor-xtask --bin harbor-ci";
    cargoInstallCommand = "install -Dm755 target/release/harbor-ci $out/bin/harbor-ci";
    nativeBuildInputs = [pkgs.clang] ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [pkgs.mold];
    nativeCheckInputs = [pkgs.git];
  }
