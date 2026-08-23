{
  pkgs,
  system,
  lib,
  bun_1_3_14,
  self,
  nixpkgs,
  treefmt-nix,
  git-hooks,
  meta,
}: let
  templateRoot = ../templates/default;
  templateFlake = builtins.readFile (templateRoot + "/flake.nix");
  templateSimit = builtins.fromTOML (builtins.readFile (templateRoot + "/simit.toml"));
  templateTreefmt = builtins.readFile (templateRoot + "/nix/treefmt.nix");
  templateHooks = builtins.readFile (templateRoot + "/nix/pre-commit.nix");
in
  assert templateSimit.flake
  == {
    scope = "full";
    mode = "custom";
    backend = "generic";
  };
  assert pkgs.lib.hasInfix "treefmt-nix.follows" templateFlake;
  assert pkgs.lib.hasInfix "git-hooks.follows" templateFlake;
  assert pkgs.lib.hasInfix "treefmtEval.config.build.check self" templateFlake;
  assert pkgs.lib.hasInfix "pre-commit-check.shellHook" templateFlake;
  assert pkgs.lib.hasInfix "\"*.ts\"" templateTreefmt;
  assert pkgs.lib.hasInfix "\"*.json\"" templateTreefmt;
  assert pkgs.lib.hasInfix "treefmt =" templateHooks;
  assert pkgs.lib.hasInfix "nix-flake-check" templateHooks;
    {
      bun-package-manager-version = let
        version = lib.bun.readPackageManagerVersion {
          packageJson = {
            packageManager = "bun@1.3.14";
          };
        };
      in
        assert version == "1.3.14";
          pkgs.runCommand "harbor-js-bun-package-manager-version" {} ''
            mkdir -p $out
            echo ok > $out/result
          '';

      bun-version = pkgs.runCommand "harbor-js-bun-version-${system}" {} ''
        test "$(${bun_1_3_14}/bin/bun --version)" = "1.3.14"
        mkdir -p $out
        echo ok > $out/result
      '';

      template-default = meta.templateTests.mkCheck {
        inherit pkgs system;
        flakeNix = ../templates/default/flake.nix;
        inputs = {
          inherit nixpkgs treefmt-nix git-hooks;
          harbor-js = self;
        };
        requiredFiles = [
          "flake.nix"
          "package.json"
          "index.ts"
          "simit.toml"
          "nix/treefmt.nix"
          "nix/pre-commit.nix"
        ];
        requiredInputs = ["harbor-js" "treefmt-nix" "git-hooks"];
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
        or (throw "harbor-js checks: no Bun compile target for `${system}`");
      in
        pkgs.runCommand "harbor-js-bun-compile-smoke-${system}" {nativeBuildInputs = [bun_1_3_14];} ''
          cat > hello.ts <<'EOF'
          console.log("hello")
          EOF

          bun build --compile --target ${pkgs.lib.escapeShellArg target} hello.ts --outfile hello
          test "$(${bun_1_3_14.passthru.fhsRunner}/bin/bun-fhs-run ./hello)" = "hello"

          mkdir -p $out
          echo ok > $out/result
        '';
    }
