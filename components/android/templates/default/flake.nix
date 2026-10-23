{
  description = "Android project — powered by harbor-android";

  inputs = {
    harbor.url = "git+https://github.com/caniko/harbor.git?ref=feat/harbor-monorepo-components&rev=a623c2db712a823f75b29523940fc7432dea49aa";
    nixpkgs.follows = "harbor/nixpkgs";
    treefmt-nix.follows = "harbor/treefmt-nix";
  };

  outputs = {
    nixpkgs,
    harbor,
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
      androidSdk =
        (harbor.lib.android.mkAndroidSdk {
          inherit pkgs;
          platformVersions = ["34"];
          buildToolsVersions = ["34.0.0"];
          ndkVersions = [androidNdkVersion];
        }).androidsdk;
    in {
      android = harbor.lib.android.mkAndroidDevShell {
        inherit pkgs androidSdk;
        ndkVersion = androidNdkVersion;
      };
    };
  in {
    formatter = nixpkgs.lib.genAttrs systems (system:
      (treefmt-nix.lib.evalModule nixpkgs.legacyPackages.${system} {
        imports = [
          harbor.treefmtModules.core-nix
          harbor.treefmtModules.core-toml
          harbor.treefmtModules.android-java
          harbor.treefmtModules.android-kotlin
        ];
        projectRootFile = "flake.nix";
      }).config.build.wrapper);

    devShells = nixpkgs.lib.genAttrs systems (system: {
      android = (forSystem system).android;
    });
  };
}
