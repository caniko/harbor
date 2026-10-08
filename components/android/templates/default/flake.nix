{
  description = "Android project — powered by harbor-android";

  inputs = {
    harbor-android.url = "github:caniko/harbor-android/trunk";
    nixpkgs.follows = "harbor-android/nixpkgs";
    treefmt-nix.follows = "harbor-android/treefmt-nix";
  };

  outputs = {
    nixpkgs,
    harbor-android,
    treefmt-nix,
    ...
  }: let
    systems = [
      "x86_64-linux"
      "aarch64-linux"
    ];
    forSystem = system: let
      pkgs = import nixpkgs {inherit system;};
      androidNdkVersion = "29.0.14206865";
      androidSdk = harbor-android.lib.mkAndroidSdk {
        inherit pkgs;
        platformVersions = ["34"];
        buildToolsVersions = ["34.0.0"];
        ndkVersions = [androidNdkVersion];
      };
    in {
      android = harbor-android.lib.mkAndroidDevShell {
        inherit pkgs androidSdk;
        ndkVersion = androidNdkVersion;
      };
    };
  in {
    formatter = nixpkgs.lib.genAttrs systems (system:
      (treefmt-nix.lib.evalModule nixpkgs.legacyPackages.${system} {
        imports = [
          harbor-android.inputs.harbor-meta.treefmtModules.nix
          harbor-android.inputs.harbor-meta.treefmtModules.toml
          harbor-android.treefmtModules.java
          harbor-android.treefmtModules.kotlin
        ];
        projectRootFile = "flake.nix";
      }).config.build.wrapper);

    devShells = nixpkgs.lib.genAttrs systems (system: {
      android = (forSystem system).android;
    });
  };
}
