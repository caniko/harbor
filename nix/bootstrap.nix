# Bootstrap tools are independent source inputs, built with Harbor's shared
# Rust engine. Their own flakes must not reintroduce predecessor Harbors.
{
  self,
  inputs,
  pkgs,
}: let
  toolchain = self.lib.rust.mkToolchain {
    inherit pkgs;
    toolchainProfile = "nightly";
    cache.enable = false;
  };
  inherit (toolchain) craneLib;
  source = pkgs.lib.cleanSourceWith {
    src = inputs.simit.outPath;
    filter = path: type: let
      pathString = toString path;
    in
      craneLib.filterCargoSources path type
      || builtins.elem pathString (map (name: "${toString inputs.simit.outPath}/${name}") ["README.md" "LICENSE" "CHANGELOG.md"])
      || pkgs.lib.hasInfix "/src/review/assets/" pathString
      || pkgs.lib.hasSuffix "/src/monorepo/native_release.py" pathString
      || pkgs.lib.hasInfix "/tests/fixtures/" pathString
      || pkgs.lib.hasSuffix "ci-actions.json" pathString
      || pkgs.lib.hasSuffix "/.github" pathString
      || pkgs.lib.hasSuffix "/.github/workflows" pathString
      || pkgs.lib.hasInfix "/.github/workflows/" pathString
      || pkgs.lib.hasSuffix "/.simit" pathString
      || pkgs.lib.hasSuffix "/.simit/workflows" pathString
      || pkgs.lib.hasInfix "/.simit/workflows/" pathString;
  };
  commonArgs = {
    src = source;
    strictDeps = true;
    cargoExtraArgs = "--all-features";
    nativeBuildInputs = [pkgs.perl];
  };
  preCommitBin = pkgs.runCommand "pre-commit-bin" {} ''
    mkdir -p "$out/bin"
    ln -s ${pkgs.pre-commit}/bin/pre-commit "$out/bin/pre-commit"
  '';
  projectArgs = {
    pname = "harbor-bootstrap-plinth-project";
    src = inputs.plinth-source.outPath;
    strictDeps = true;
    cargoExtraArgs = "--locked --package plinth-project --bin plinth-project";
    doCheck = false;
  };
in {
  simit = craneLib.buildPackage (commonArgs
    // {
      cargoArtifacts = craneLib.buildDepsOnly commonArgs;
      nativeBuildInputs = commonArgs.nativeBuildInputs ++ [preCommitBin];
      nativeCheckInputs = [pkgs.git pkgs.gnupg pkgs.nodejs pkgs.jq pkgs.python3 pkgs.actionlint];
    });
  plinthProject = craneLib.buildPackage (projectArgs
    // {
      cargoArtifacts = craneLib.buildDepsOnly projectArgs;
    });
}
