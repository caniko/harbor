{
  pkgs,
  lib,
  meta,
}: let
  inherit (lib) node;
  packageJson.packageManager = "pnpm@${pkgs.pnpm_10.version}";
  toolchain = node.mkNodeToolchain {inherit pkgs packageJson;};
  shell = node.mkNodeDevShell {
    inherit pkgs packageJson;
    extraEnv.HARBOR_NODE_CHECK = "configured";
  };
  shellWithoutLsp = node.mkNodeDevShell {
    inherit pkgs packageJson;
    opencodeLsp.enable = false;
  };
  hasPackage = shell: builtins.elem pkgs.typescript-language-server shell.passthru.devShellSpec.packages;
  fails = value: !(builtins.tryEval (builtins.deepSeq value true)).success;
  # A real consumer still pins pnpm 9. Exercise the Nixpkgs builder override
  # and a frozen offline install, rather than only testing the default tool.
  pinnedPnpm = node.mkPnpmPackage {
    inherit pkgs;
    version = "9.15.4";
    hash = "sha512-stwg4vxys+GISEWbNzWaMgZGY+VielHkx0ssKd2OjgSRSDw6u0B4nP1Xi/Ni+2uoJhsF8Dh9dnku1uI+o7G2oA==";
  };
  pinnedToolchain = node.mkNodeToolchain {
    inherit pkgs;
    pnpm = pinnedPnpm;
    packageJson.packageManager = "pnpm@9.15.4";
  };
in {
  node-contract = assert node.readPnpmVersion {inherit packageJson;} == pkgs.pnpm_10.version;
  assert fails (node.readPnpmVersion {packageJson.packageManager = "pnpm@latest";});
  assert fails (node.readPnpmVersion {packageJson.packageManager = "yarn@1.22.0";});
  assert fails (node.readPnpmVersion {packageJson = {};});
  assert fails (node.mkNodeToolchain {
    inherit pkgs;
    packageJson.packageManager = "pnpm@0.0.0";
  });
  assert toolchain.nodejs == pkgs.nodejs;
  assert !(pkgs.lib.hasInfix "pnpm install" shell.passthru.devShellSpec.shellHook);
  # OpenCode gets a TypeScript server (and other editors find it on PATH)
  # unless the profile is disabled.
  assert hasPackage shell;
  assert pkgs.lib.hasInfix "OPENCODE_CONFIG_CONTENT" shell.passthru.devShellSpec.shellHook;
  assert !(hasPackage shellWithoutLsp);
  assert !(pkgs.lib.hasInfix "OPENCODE_CONFIG_CONTENT" shellWithoutLsp.passthru.devShellSpec.shellHook);
    pkgs.runCommand "harbor-js-node-contract" {
      nativeBuildInputs = toolchain.packages;
    } ''
      test "$(node --version)" = "v${toolchain.nodejs.version}"
      test "$(pnpm --version)" = "${toolchain.pnpm.version}"
      mkdir -p "$out"
      echo ok > "$out/result"
    '';

  node-dev-shell = meta.devShellTests.mkCheck {
    inherit pkgs shell;
    name = "harbor-js-node-dev-shell";
    commands = ["node" "npm" "pnpm" "typescript-language-server"];
    env.HARBOR_NODE_CHECK = "configured";
  };

  # The merged OpenCode config points the typescript server at the binary the
  # shell puts on PATH, and keeps an existing caller config.
  node-opencode-typescript-lsp =
    pkgs.runCommand "harbor-js-node-opencode-typescript-lsp" {
      nativeBuildInputs = shell.nativeBuildInputs ++ [pkgs.jq];
    } ''
        export HOME="$TMPDIR" XDG_CACHE_HOME="$TMPDIR"
        export OPENCODE_CONFIG_CONTENT='{"lsp":{"caller":{"command":["caller-lsp"]}}}'
        ${shell.passthru.devShellSpec.shellHook}
        server="$(jq -er '.lsp.typescript.command[0]' <<<"$OPENCODE_CONFIG_CONTENT")"
        test -x "$server"
        test "$server" = "$(command -v typescript-language-server)"
        jq -e '.lsp.caller.command == ["caller-lsp"]' <<<"$OPENCODE_CONFIG_CONTENT" >/dev/null
      jq -e '.lsp.rust.command == ["rust-analyzer"]' <<<"$OPENCODE_CONFIG_CONTENT" >/dev/null
        mkdir -p "$out"
        echo ok > "$out/result"
    '';

  pnpm-pinned-install = pkgs.runCommand "harbor-js-pnpm-pinned-install" ({
      nativeBuildInputs = pinnedToolchain.packages;
    }
    // pinnedToolchain.env) ''
    export HOME="$TMPDIR/home"
    mkdir -p "$HOME" fixture
    cd fixture
    cp ${../templates/node/package.json} package.json
    cp ${../templates/node/index.test.js} index.test.js
    cat > pnpm-lock.yaml <<'LOCK'
    lockfileVersion: '9.0'

    settings:
      autoInstallPeers: true
      excludeLinksFromLockfile: false

    importers:

      .: {}
    LOCK
    cp pnpm-lock.yaml original-lock.yaml
    test "$(pnpm --version)" = '9.15.4'
    pnpm install --frozen-lockfile --offline --ignore-scripts
    diff -u original-lock.yaml pnpm-lock.yaml
    cmp pnpm-lock.yaml original-lock.yaml
    pnpm test
    mkdir -p "$out"
    echo ok > "$out/result"
  '';
}
