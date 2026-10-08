{
  crane,
  osxcross,
  meta-harbor ? null,
  nixBundle ? null,
}: let
  devShellLib = import ./dev-shell.nix;
  adapterLib = import ./adapter.nix;
  minisignLib = import ./minisign.nix;
  mkCargoConfig = import ./cargo-config.nix;
  mkBuildCachePolicy = (import ./build-cache.nix {}).mkBuildCachePolicy;
  mkToolchain = import ./toolchain.nix {
    inherit crane mkBuildCachePolicy mkCargoConfig;
  };
  hardeningProfiles = import ./hardening-profiles.nix;
  mkSccacheLib = import ./sccache.nix {};
  packageTests =
    if meta-harbor != null
    then meta-harbor.packageTests
    else throw "rs-harbor: package-test helpers require the meta-harbor flake input";
  dioxusLib = import ./dioxus.nix {inherit packageTests;};
in {
  inherit mkBuildCachePolicy mkToolchain hardeningProfiles packageTests mkCargoConfig;
  buildContract = import ./build-contract.nix {};
  opencode =
    if meta-harbor != null
    then meta-harbor.opencode
    else throw "rs-harbor: opencode helpers require the meta-harbor flake input";

  mkRustNativeBuildInputs = import ./rust-native-build-inputs.nix;
  mkCross = import ./cross.nix {inherit osxcross;};
  mkCrossPackages = import ./cross-packages.nix {inherit mkToolchain;};
  mkCrossPackageOutputs = import ./cross-package-outputs.nix {
    mkCrossPackages = import ./cross-packages.nix {inherit mkToolchain;};
  };
  mkBinaryRelease = args:
    (import ./binary-release.nix {pkgs = args.pkgs;}).mkBinaryRelease
    (builtins.removeAttrs args ["pkgs"]);
  mkReleaseBinaryPackage = args:
    (import ./binary-release.nix {pkgs = args.pkgs;}).mkReleaseBinaryPackage
    (builtins.removeAttrs args ["pkgs"]);
  mkPortableBinaryRelease = args:
    (import ./portable-release.nix {
      pkgs = args.pkgs;
      bundlers =
        if args ? bundlers
        then args.bundlers
        else if nixBundle != null
        then builtins.mapAttrs (_: value: value.nix-bundle) nixBundle.bundlers
        else throw "rs-harbor: mkPortableBinaryRelease requires nixBundle or bundlers";
    }).mkPortableBinaryRelease
    (builtins.removeAttrs args ["pkgs" "bundlers"]);
  mkPortableReleaseBinaryPackage = args:
    (import ./portable-release.nix {
      pkgs = args.pkgs;
      bundlers = {};
    }).mkPortableReleaseBinaryPackage
    (builtins.removeAttrs args ["pkgs"]);
  mkReleaseArtifact = args:
    (import ./release-artifacts.nix {pkgs = args.pkgs;}).mkReleaseArtifact
    (builtins.removeAttrs args ["pkgs"]);
  mkReleaseArchive = args:
    (import ./release-artifacts.nix {pkgs = args.pkgs;}).mkReleaseArchive
    (builtins.removeAttrs args ["pkgs"]);
  mkReleaseBundle = args:
    (import ./release-artifacts.nix {pkgs = args.pkgs;}).mkReleaseBundle
    (builtins.removeAttrs args ["pkgs"]);
  mkSteamRuntimeTools = import ./steam-runtime.nix;
  mkGpuRenderPin = import ./gpu-render-pin.nix;
  mkMacosUniversalStager = import ./macos-staging.nix;
  mkOsxcrossHooks = import ./osxcross-hooks.nix;
  mkWindowsMsvcDevShell = import ./windows-msvc-shell.nix;
  inherit (devShellLib) mkDevShell mkDocsShell mkDevShells mkProjectCliShellTools mkPkgConfigEnv;
  inherit (adapterLib) mkAdapter isHarborAdapter;
  inherit (minisignLib) mkMinisignSign mkMinisignVerify;

  mkWasmToolchain = import ./wasm-toolchain.nix {inherit crane;};
  mkGradlePackage = import ./gradle-package.nix;
  mkJetBrainsPlugin = import ./jetbrains-plugin.nix;
  mkJetBrainsSigningMaterial = import ./jetbrains-signing.nix;
  mkTrunkPackage = import ./trunk.nix {inherit crane packageTests;};
  inherit (dioxusLib) mkDioxusPackage mkDioxusWebPackage mkDioxusFullstackPackage;
  mkDioxusCli = import ./dioxus-cli.nix;
  mkDioxusBuildPlan = import ./dioxus-build-plan.nix;
  mkDioxusAssetLinker = import ./dioxus-asset-linker.nix;
  resolveWasmBindgenCli = import ./wasm-bindgen.nix;
  mkRustServiceModule = import ./nixos-rust-service.nix {
    inherit hardeningProfiles;
  };
  mkRustCommandServiceModule = import ./nixos-rust-command-service.nix {
    inherit hardeningProfiles;
  };

  findLocalMavenCache = import ./android-maven-cache.nix;
  fetchMavenCache = import ./fetch-maven-cache.nix;
  mkAtticPush = import ./attic-push.nix;
  mkAndroidApk = import ./android-apk.nix {inherit packageTests;};
  mkAndroidApkDevBuilder = import ./android-apk-dev-builder.nix {inherit packageTests;};
  mkAndroidFlavorTable = import ./android-flavor-table.nix {inherit packageTests;};
  mkAppImage = import ./appimage.nix {inherit packageTests;};
  mkPackageArtifactBuilder = import ./package-artifact-builder.nix {inherit packageTests;};
  mkChocoPackage = import ./choco-package.nix {inherit packageTests;};
  mkChocoTestEnvironment = import ./choco-test-environment.nix {inherit packageTests;};
  mkPackageTestPlan = import ./package-test-plan.nix {inherit packageTests;};
  mkCoprSpec = import ./copr-spec.nix {inherit packageTests;};
  mkDebPackage = import ./deb-package.nix {inherit packageTests;};
  mkFlatpakManifest = import ./flatpak-manifest.nix {inherit packageTests;};
  mkHomebrewFormula = import ./homebrew-formula.nix {inherit packageTests;};
  mkScoopManifest = import ./scoop-manifest.nix {inherit packageTests;};
  mkSccacheEnv = mkSccacheLib; # backward compat: rs-harbor.lib.mkSccacheEnv.mkSccacheEnv { ... }
  mkSccacheCraneEnv = mkSccacheLib.mkSccacheCraneEnv;
  wrapRustPackageWithSccache = mkSccacheLib.wrapRustPackageWithSccache;
}
