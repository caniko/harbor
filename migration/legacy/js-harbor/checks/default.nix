{
  pkgs,
  system,
  lib,
  bun_1_3_14,
  self,
  nixpkgs,
  meta,
}:
{
  bun-package-manager-version = let
    version = lib.bun.readPackageManagerVersion {
      packageJson = {
        packageManager = "bun@1.3.14";
      };
    };
  in
    assert version == "1.3.14";
      pkgs.runCommand "js-harbor-bun-package-manager-version" {} ''
        mkdir -p $out
        echo ok > $out/result
      '';

  bun-version = pkgs.runCommand "js-harbor-bun-version-${system}" {} ''
    test "$(${bun_1_3_14}/bin/bun --version)" = "1.3.14"
    mkdir -p $out
    echo ok > $out/result
  '';

  template-default = meta.templateTests.mkCheck {
    inherit pkgs system;
    flakeNix = ../templates/default/flake.nix;
    inputs = {
      inherit nixpkgs;
      js-harbor = self;
    };
    requiredFiles = ["flake.nix" "package.json" "index.ts"];
    requiredInputs = ["js-harbor"];
    commands = ["bun"];
    env.BUN_VERSION = "1.3.14";
    inherit (meta) devShellTests;
  };
}
// pkgs.lib.optionalAttrs pkgs.stdenvNoCC.hostPlatform.isLinux {
  bun-compile-smoke = let
    targetBySystem = {
      aarch64-linux = "bun-linux-aarch64";
      x86_64-linux = "bun-linux-x64";
    };
    target =
      targetBySystem.${system}
        or (throw "js-harbor checks: no Bun compile target for `${system}`");
  in
    pkgs.runCommand "js-harbor-bun-compile-smoke-${system}" {nativeBuildInputs = [bun_1_3_14];} ''
      cat > hello.ts <<'EOF'
      console.log("hello")
      EOF

      bun build --compile --target ${pkgs.lib.escapeShellArg target} hello.ts --outfile hello
      test "$(${bun_1_3_14.passthru.fhsRunner}/bin/bun-fhs-run ./hello)" = "hello"

      mkdir -p $out
      echo ok > $out/result
    '';
}
