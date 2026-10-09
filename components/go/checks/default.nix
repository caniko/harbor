{
  pkgs,
  self,
  nixpkgs,
  harbor-meta,
}: let
  inherit (harbor-meta.lib) devShellTests templateTests;
  system = pkgs.stdenv.hostPlatform.system;
  template = templateTests.eval {
    flakeNix = ../templates/default/flake.nix;
    inputs = {
      harbor = self.inputs.harborRoot;
      inherit nixpkgs;
      treefmt-nix = self.inputs.treefmt-nix;
    };
  };
  toolchain = self.lib.mkGoToolchain {inherit pkgs;};
  embedded = self.lib.mkGoPackage {
    inherit pkgs toolchain;
    pname = "harbor-go-generated-assets";
    version = "1.0";
    src = ../templates/default;
    vendorHash = null;
    subPackages = ["."];
    env.CGO_ENABLED = "0";
    ldflags = ["-X main.version=fixture"];
    postConfigure = ''
      printf 'generated assets\n' > greeting.txt
    '';
    doCheck = true;
    checkPhase = ''
      runHook preCheck
      test "$(go run -ldflags='-X main.version=fixture' .)" = 'fixture: generated assets'
      runHook postCheck
    '';
  };
in {
  dev-shell = devShellTests.mkCheck {
    inherit pkgs;
    name = "harbor-go-dev-shell";
    shell = self.devShells.${system}.default;
    commands = ["go" "gopls" "golangci-lint" "cc" "treefmt"];
    env = {
      GOTOOLCHAIN = "local";
      CGO_ENABLED = "1";
    };
  };
  template-default = templateTests.mkCheck {
    inherit pkgs system devShellTests;
    flakeNix = ../templates/default/flake.nix;
    inputs = {
      harbor = self.inputs.harborRoot;
      inherit nixpkgs;
      treefmt-nix = self.inputs.treefmt-nix;
    };
    requiredFiles = ["flake.nix" "go.mod" "main.go" "main_test.go" "greeting.txt"];
    requiredInputs = ["harbor"];
    commands = ["go" "gopls" "golangci-lint"];
    env.GOTOOLCHAIN = "local";
  };
  template-package = template.packages.${system}.default;
  generated-assets = pkgs.runCommand "harbor-go-generated-assets-smoke" {} ''
    test "$(${embedded}/bin/hello)" = 'fixture: generated assets'
    touch "$out"
  '';
  cgo-race = self.lib.mkGoPackage {
    inherit pkgs toolchain;
    pname = "harbor-go-cgo-race";
    version = "1.0";
    src = ./fixtures/cgo;
    vendorHash = null;
    subPackages = ["."];
    env.CGO_ENABLED = "1";
    checkFlags = ["-race"];
    doCheck = true;
  };
  tool-versions =
    pkgs.runCommand "harbor-go-tool-versions" {
      nativeBuildInputs = [toolchain.go toolchain.gopls toolchain.golangciLint];
      env = {
        GOTOOLCHAIN = "local";
        CGO_ENABLED = "0";
        GOPROXY = "off";
      };
    } ''
      go version | grep -F 'go${toolchain.go.version}'
      gopls version
      test "$(golangci-lint version --short)" = '2.13.1'
      go version -m "$(command -v gopls)" | grep -F 'go${toolchain.go.version}'
      go version -m "$(command -v golangci-lint)" | grep -F 'go${toolchain.go.version}'
      export HOME="$TMPDIR" GOLANGCI_LINT_CACHE="$TMPDIR/lint-cache" GOCACHE="$TMPDIR/go-cache"
      cp -R ${../templates/default} source
      chmod -R u+w source
      cd source
      golangci-lint run --timeout 2m
      touch "$out"
    '';
}
