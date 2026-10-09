{
  description = "A minimal Go module using harbor-go";
  inputs = {
    harbor.url = "git+https://github.com/caniko/harbor.git?ref=feat/harbor-monorepo-components&rev=7d99eb50c52d0a941e2996b97c469b32a7657ef4";
    nixpkgs.follows = "harbor/nixpkgs";
    treefmt-nix.follows = "harbor/treefmt-nix";
  };
  outputs = {
    self,
    harbor,
    nixpkgs,
    treefmt-nix,
    ...
  }: let
    systems = ["x86_64-linux" "aarch64-linux" "aarch64-darwin"];
    forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    format = pkgs:
      treefmt-nix.lib.evalModule pkgs {
        imports = [harbor.treefmtModules.core-nix harbor.treefmtModules.go-go];
        projectRootFile = "flake.nix";
      };
  in {
    packages = forAllSystems (pkgs: {
      default = harbor.lib.go.mkGoPackage {
        inherit pkgs;
        pname = "hello";
        version = "0.1.0";
        src = self.outPath;
        vendorHash = null;
        subPackages = ["."];
        env.CGO_ENABLED = "0";
        doCheck = true;
        meta.mainProgram = "hello";
      };
    });
    checks = forAllSystems (pkgs: {
      package = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      formatting = (format pkgs).config.build.check self;
    });
    devShells = forAllSystems (pkgs: {
      default = harbor.lib.go.mkGoDevShell {
        inherit pkgs;
        packages = [(format pkgs).config.build.wrapper];
      };
    });
    formatter = forAllSystems (pkgs: (format pkgs).config.build.wrapper);
  };
}
