{
  crane,
  osxcross,
  harbor-meta ? null,
  nixBundle ? null,
  harbor-android ? null,
}: let
  devShellLib = import ./dev-shell.nix {
    metaDevShell =
      if harbor-meta != null
      then harbor-meta.devShell
      else null;
    opencodeProfiles = opencodeLib.profiles;
  };
  adapterLib = import ./adapter.nix;
  opencodeLib =
    if harbor-meta != null
    then import ./opencode.nix {inherit harbor-meta;}
    else throw "harbor-rs: opencode helpers require the harbor-meta flake input";
  metaShellTools =
    if harbor-meta != null
    then harbor-meta.devShell
    else throw "harbor-rs: shell helpers require the harbor-meta flake input";
  minisignLib = import ./minisign.nix;
  timezone =
    if harbor-meta != null && harbor-meta ? timezone
    then harbor-meta.timezone
    else throw "harbor-rs: timezone helpers require an updated harbor-meta flake input";
  mkCargoConfig = import ./cargo-config.nix;
  inherit ((import ./build-cache.nix {})) mkBuildCachePolicy;
  mkToolchain = import ./toolchain.nix {
    inherit crane mkBuildCachePolicy mkCargoConfig;
  };
  hardeningProfiles = import ./hardening-profiles.nix;
  mkSccacheLib = import ./sccache.nix {};
  packageTests =
    if harbor-meta != null
    then harbor-meta.packageTests
    else throw "harbor-rs: package-test helpers require the harbor-meta flake input";
  templateTests =
    if harbor-meta != null
    then harbor-meta.templateTests
    else throw "harbor-rs: template-test helpers require the harbor-meta flake input";
  devShellTests =
    if harbor-meta != null
    then harbor-meta.devShellTests
    else throw "harbor-rs: dev-shell-test helpers require the harbor-meta flake input";
  dioxusLib = import ./dioxus.nix {inherit packageTests;};
in {
  inherit mkBuildCachePolicy mkToolchain hardeningProfiles packageTests templateTests devShellTests mkCargoConfig;
  buildContract = import ./build-contract.nix {};
  inherit timezone;
  opencode =
    if harbor-meta != null
    then harbor-meta.opencode
    else throw "harbor-rs: opencode helpers require the harbor-meta flake input";
  opencodeRust = opencodeLib;
  hooks = import ./hooks.nix {inherit harbor-meta;};

  mkRustNativeBuildInputs = import ./rust-native-build-inputs.nix;
  mkCross = import ./cross.nix {inherit osxcross;};
  mkCrossPackages = import ./cross-packages.nix {inherit mkToolchain;};
  mkCrossPackageOutputs = import ./cross-package-outputs.nix {
    mkCrossPackages = import ./cross-packages.nix {inherit mkToolchain;};
  };
  mkBinaryRelease = args:
    (import ./binary-release.nix {
      inherit (args) pkgs;
      inherit timezone;
    }).mkBinaryRelease
    (builtins.removeAttrs args ["pkgs"]);
  mkReleaseBinaryPackage = args:
    (import ./binary-release.nix {inherit (args) pkgs;}).mkReleaseBinaryPackage
    (builtins.removeAttrs args ["pkgs"]);
  mkPortableBinaryRelease = args:
    (import ./portable-release.nix {
      inherit (args) pkgs;
      inherit timezone;
      bundlers =
        args.bundlers or (
          if nixBundle != null
          then builtins.mapAttrs (_: value: value.nix-bundle) nixBundle.bundlers
          else throw "harbor-rs: mkPortableBinaryRelease requires nixBundle or bundlers"
        );
    }).mkPortableBinaryRelease
    (builtins.removeAttrs args ["pkgs" "bundlers"]);
  mkPortableReleaseBinaryPackage = args:
    (import ./portable-release.nix {
      inherit (args) pkgs;
      bundlers = {};
    }).mkPortableReleaseBinaryPackage
    (builtins.removeAttrs args ["pkgs"]);
  mkReleaseArtifact = args:
    (import ./release-artifacts.nix {inherit (args) pkgs;}).mkReleaseArtifact
    (builtins.removeAttrs args ["pkgs"]);
  mkReleaseArchive = args:
    (import ./release-artifacts.nix {
      inherit (args) pkgs;
      inherit timezone;
    }).mkReleaseArchive
    (builtins.removeAttrs args ["pkgs"]);
  mkReleaseBundle = args:
    (import ./release-artifacts.nix {inherit (args) pkgs;}).mkReleaseBundle
    (builtins.removeAttrs args ["pkgs"]);
  mkSteamRuntimeTools = import ./steam-runtime.nix;
  mkGpuRenderPin = import ./gpu-render-pin.nix;
  mkDevSandbox = import ./dev-sandbox.nix;
  mkMacosUniversalStager = import ./macos-staging.nix;
  mkOsxcrossHooks = import ./osxcross-hooks.nix;
  mkWindowsMsvcDevShell = import ./windows-msvc-shell.nix {inherit metaShellTools;};
  inherit (devShellLib) mkDevShell mkDocsShell mkDevShells;
  inherit (metaShellTools) mkProjectCliShellTools mkPkgConfigEnv;
  inherit (adapterLib) mkAdapter isHarborAdapter;
  inherit (minisignLib) mkMinisignSign mkMinisignVerify;

  mkWasmToolchain = import ./wasm-toolchain.nix {inherit crane;};
  mkGradlePackage = import ./gradle-package.nix;
  mkGradleMavenProxyInitScript = import ./maven-cache.nix;
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

  fetchMavenCache = import ./fetch-maven-cache.nix;
  mkAtticPush = import ./attic-push.nix;
  inherit
    (
      if harbor-android != null
      then harbor-android
      else throw "harbor-rs: Android helpers require the harbor-android flake input"
    )
    findLocalMavenCache
    mkAndroidApk
    mkAndroidApkDevBuilder
    mkAndroidFlavorTable
    mkAndroidSdk
    ;
  mkAndroidDevShell = args @ {
    pkgs,
    timeZone ? null,
    ...
  }:
    timezone.withShell ({
        inherit pkgs;
        shell = harbor-android.mkAndroidDevShell (builtins.removeAttrs args ["timeZone"]);
      }
      // pkgs.lib.optionalAttrs (timeZone != null) {inherit timeZone;});
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
  mkSccacheEnv = mkSccacheLib; # backward compat: harbor-rs.lib.mkSccacheEnv.mkSccacheEnv { ... }
  inherit (mkSccacheLib) mkSccacheCraneEnv;
  inherit (mkSccacheLib) wrapRustPackageWithSccache;
}
