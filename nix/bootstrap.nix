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
  # simit's own actionlint: 1.7.12 plus the native concurrency-queue parser patch.
  actionlint = pkgs.callPackage "${inputs.simit}/nix/actionlint.nix" {};
  source = pkgs.lib.cleanSourceWith {
    src = inputs.simit.outPath;
    filter = path: type: let
      pathString = toString path;
    in
      craneLib.filterCargoSources path type
      || builtins.elem pathString (map (name: "${toString inputs.simit.outPath}/${name}") ["README.md" "LICENSE" "CHANGELOG.md" "release-plz.toml"])
      || pkgs.lib.hasInfix "/src/review/assets/" pathString
      # Scripts embedded with include_str! (render helpers, release/provision).
      || (pkgs.lib.hasInfix "/src/" pathString && (pkgs.lib.hasSuffix ".py" pathString || pkgs.lib.hasSuffix ".sh" pathString))
      || pkgs.lib.hasInfix "/tests/fixtures/" pathString
      || pkgs.lib.hasSuffix "/scripts" pathString
      || pkgs.lib.hasInfix "/scripts/qualify-" pathString
      || pkgs.lib.hasSuffix "/.simit/release-plz.py" pathString
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
    # Mirrors simit's own package: compile-time trust anchors become runtime
    # closure references; job PATH/SSL_CERT_* cannot replace them.
    SIMIT_TRUSTED_GIT = "${pkgs.git}/bin/git";
    SIMIT_TRUSTED_CA_BUNDLE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
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
      nativeCheckInputs = [pkgs.git pkgs.gnupg pkgs.nodejs pkgs.jq pkgs.python3 actionlint] ++ pkgs.lib.optionals pkgs.stdenv.isLinux [pkgs.bubblewrap pkgs.nix pkgs.sqlite];
      # As upstream: namespace integration is qualified outside the build sandbox.
      cargoTestExtraArgs = "-- --skip commands::projects_update::engine::tests::namespace_";
      # Simit's timing-bound integration tests flake on hosted macOS runners
      # (harbor #21: a 3.1 s pipe release, then ProbeTimedOut for
      # AttemptsExhausted). Linux jobs still run the suite on the same source.
      doCheck = !pkgs.stdenv.hostPlatform.isDarwin;
    });
  plinthProject = craneLib.buildPackage (projectArgs
    // {
      cargoArtifacts = craneLib.buildDepsOnly projectArgs;
    });
}
