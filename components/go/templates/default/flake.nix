{
  description = "A minimal Go module using harbor-go";
  inputs = {
    harbor-go.url = "github:caniko/harbor-go/trunk";
    nixpkgs.follows = "harbor-go/nixpkgs";
    treefmt-nix.follows = "harbor-go/treefmt-nix";
  };
  outputs = {
    self,
    harbor-go,
    nixpkgs,
    treefmt-nix,
    ...
  }: let
    systems = ["x86_64-linux" "aarch64-linux" "aarch64-darwin"];
    forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    format = pkgs:
      treefmt-nix.lib.evalModule pkgs {
        imports = [harbor-go.inputs.harbor-meta.treefmtModules.nix harbor-go.treefmtModules.go];
        projectRootFile = "flake.nix";
      };
  in {
    packages = forAllSystems (pkgs: {
      default = harbor-go.lib.mkGoPackage {
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
      default = harbor-go.lib.mkGoDevShell {
        inherit pkgs;
        packages = [(format pkgs).config.build.wrapper];
      };
    });
    formatter = forAllSystems (pkgs: (format pkgs).config.build.wrapper);
  };
}
