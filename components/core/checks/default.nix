{
  pkgs,
  lib,
  harborOpencode,
  nixpkgs,
}: let
  merged = lib.devShell.merge [
    {
      packages = [pkgs.hello];
      env.A = "1";
      shellHook = "echo a";
    }
    {
      packages = [pkgs.jq];
      env.A = "2";
      shellHook = "echo b";
    }
  ];
  helloShell = lib.devShell.mkShell {
    inherit pkgs;
    packages = [pkgs.hello];
    env.HELLO_SHELL = "1";
    extraShellHook = "true";
  };
  derivationManifest = let
    fixture =
      pkgs.runCommand "harbor-meta-derivation-manifest-fixture" {
        outputs = ["out" "dev"];
      } ''
        mkdir -p "$out" "$dev"
      '';
  in
    lib.flake.mkDerivationManifest {
      inherit (pkgs.stdenv.hostPlatform) system;
      targets = {
        default = fixture;
        inherit (fixture) dev;
        multiple = "${fixture}${fixture.dev}";
      };
    };
  staticTarget = builtins.tryEval (builtins.deepSeq (lib.flake.mkDerivationManifest {
      inherit (pkgs.stdenv.hostPlatform) system;
      targets.static = "/tmp/static";
    })
    true);
  ambiguousTarget = builtins.tryEval (builtins.deepSeq (lib.flake.mkDerivationManifest {
      inherit (pkgs.stdenv.hostPlatform) system;
      targets.ambiguous = "${pkgs.hello}${pkgs.jq}";
    })
    true);

  marker = lib.opencode.formatPolicy.marker;
  markerValue = lib.opencode.formatPolicy.markerValue;

  # Fixture profiles use deliberately neutral signals and commands: the
  # harbor-meta engine must be language-agnostic, so nothing here mentions
  # Cargo, rust-analyzer, pyproject, or any real language toolchain. Rust-
  # specific assertions live in harbor-rs, which binds the real registry.
  fixtureProfiles = {
    alpha = {
      lsp."fixture-alpha-lsp".command = ["fixture-alpha-lsp"];
      detect = {
        files = ["ALPHA"];
        flakeMarkers = ["fixture-alpha"];
      };
      packages = _pkgs: [];
    };
    beta = {
      lsp."fixture-beta-lsp".command = ["fixture-beta-lsp"];
      detect = {
        files = ["BETA"];
        flakeMarkers = ["fixture-beta"];
      };
      packages = _pkgs: [];
    };
  };
  fixtureRegistry = lib.opencode.mkRegistry fixtureProfiles;
  fixtureCli = lib.opencode.mkCli {
    inherit pkgs;
    profiles = fixtureProfiles;
  };
  alphaLsp = lib.opencode.lspFor fixtureRegistry ["alpha"];
  fixtureLsp = lib.opencode.lspFor fixtureRegistry ["alpha" "beta"];

  # Shared assertion helpers for the runCommand scripts. stdenv's errexit
  # state is ambiguous for buildCommand, so every script opts in with an
  # explicit `set -e`; absence assertions must then be live, which bare
  # `! grep` is not (the POSIX `!` exemption makes it a no-op under
  # errexit). The `if grep; then exit 1; fi` shape taken by these helpers
  # always terminates the build on a violation.
  helpers = ''
    must_grep() {
      local file="$1" pat="$2"
      if ! grep -qF -- "$pat" "$file"; then
        echo "expected in $file: $pat" >&2
        cat "$file" >&2 || true
        exit 1
      fi
    }

    must_not_grep() {
      local file="$1" pat="$2"
      if grep -qF -- "$pat" "$file"; then
        echo "unexpected in $file: $pat" >&2
        cat "$file" >&2 || true
        exit 1
      fi
    }

    expect_die() {
      local what="$1" rc=0
      shift
      "$@" || rc=$?
      if [ "$rc" -eq 0 ]; then
        echo "$what: expected failure, got success" >&2
        exit 1
      fi
      if [ "$rc" -eq 127 ]; then
        echo "$what: command not found (test harness bug)" >&2
        exit 1
      fi
    }

    expect_detect() {
      local want="$1" dir="$2" got
      got="$(harbor-opencode detect --root "$dir")"
      if [ "$got" != "$want" ]; then
        echo "detect --root $dir: expected $want, got $got" >&2
        exit 1
      fi
    }
  '';
in
  assert merged.env.A == "2";
  assert builtins.length merged.packages == 2;
  assert pkgs.lib.hasInfix "echo a" merged.shellHook;
  assert pkgs.lib.hasInfix "echo b" merged.shellHook; {
    dev-shell-hello = lib.devShellTests.mkCheck {
      inherit pkgs;
      name = "harbor-meta-dev-shell-hello";
      shell = helloShell;
      commands = ["hello"];
      env.HELLO_SHELL = "1";
      hookContains = ["true"];
      runHook = true;
    };

    stub-template = lib.templateTests.mkCheck {
      inherit pkgs;
      inherit (pkgs.stdenv.hostPlatform) system;
      flakeNix = ../tests/fixtures/stub-template/flake.nix;
      inputs = {inherit nixpkgs;};
      requiredFiles = ["flake.nix"];
      commands = ["hello"];
      env.STUB_SHELL = "1";
      hookContains = ["echo stub"];
      inherit (lib) devShellTests;
    };

    opencode-configs = pkgs.runCommand "harbor-meta-opencode-configs" {} ''
      set -e
      ${helpers}

      cat > policy.json <<'EOF'
      ${lib.opencode.configTextFor {}}
      EOF
      cat > alpha.json <<'EOF'
      ${lib.opencode.configTextFor {lsp = alphaLsp;}}
      EOF
      cat > combo.json <<'EOF'
      ${lib.opencode.configTextFor {lsp = fixtureLsp;}}
      EOF
      cat > alpha-openpencil.json <<'EOF'
      ${lib.opencode.configTextFor {
        lsp = alphaLsp;
        openpencil = true;
      }}
      EOF

      # Policy-only: no lsp block, identity marker, deny fragment present.
      must_not_grep policy.json '"lsp"'
      must_grep policy.json '"${marker}":"${markerValue}"'
      must_grep policy.json '"alejandra *":"deny"'

      # Profile lsp blocks render verbatim; subsets select what appears.
      must_grep alpha.json '"fixture-alpha-lsp":{"command":["fixture-alpha-lsp"]}'
      must_not_grep alpha.json fixture-beta-lsp
      must_grep combo.json fixture-alpha-lsp
      must_grep combo.json fixture-beta-lsp

      # Openpencil joins the same document without policy collisions.
      must_grep alpha-openpencil.json openpencil-desktop
      must_grep alpha-openpencil.json fixture-alpha-lsp
      must_not_grep alpha-openpencil.json '"mode":"subagent"'
      must_not_grep alpha-openpencil.json '"openpencil_*":"deny"'

      mkdir -p "$out"
      echo ok > "$out/result"
    '';

    harbor-opencode-sync-check = pkgs.runCommand "harbor-meta-opencode-sync-check" {} ''
      set -e
      ${helpers}
      export PATH=${fixtureCli}/bin:$PATH

      # Detection reads only profile-declared signals; a bare Cargo.toml
      # must not select anything in the language-agnostic engine.
      mkdir -p d-alpha && touch d-alpha/ALPHA
      mkdir -p d-beta && touch d-beta/BETA
      mkdir -p d-combo && touch d-combo/ALPHA d-combo/BETA
      mkdir -p d-mark && printf 'fixture-alpha = true;\n' > d-mark/flake.nix
      mkdir -p d-cargo && touch d-cargo/Cargo.toml
      mkdir -p d-empty

      expect_detect alpha d-alpha
      expect_detect beta d-beta
      expect_detect alpha,beta d-combo
      expect_detect alpha d-mark
      expect_detect none d-cargo
      expect_detect none d-empty

      # sync renders the detected profile; check agrees.
      harbor-opencode sync --kind detect --root d-alpha
      cfg=d-alpha/.opencode/opencode.jsonc
      must_grep "$cfg" '"fixture-alpha-lsp":{"command":["fixture-alpha-lsp"]}'
      must_not_grep "$cfg" fixture-beta-lsp
      must_not_grep "$cfg" openpencil
      harbor-opencode check --kind detect --root d-alpha

      # Explicit kinds normalize to registry order: `beta,alpha` renders
      # exactly what detection produces for the same subset.
      harbor-opencode sync --kind detect --root d-combo
      rm -f d-combo/.opencode/opencode.jsonc
      harbor-opencode sync --kind beta,alpha --root d-combo
      harbor-opencode check --kind detect --root d-combo

      # Invalid kind spellings die.
      expect_die 'bogus profile' harbor-opencode sync --kind bogus --root d-empty
      expect_die 'detect in a profile list' harbor-opencode sync --kind alpha,detect --root d-empty
      expect_die 'none combined with a profile' harbor-opencode sync --kind none,alpha --root d-empty
      expect_die 'empty token in --kind' harbor-opencode sync --kind alpha,,beta --root d-empty

      # The openpencil flag flips the render in both directions.
      harbor-opencode sync --kind alpha --openpencil --root d-op
      ocfg=d-op/.opencode/opencode.jsonc
      must_grep "$ocfg" openpencil-desktop
      must_grep "$ocfg" fixture-alpha-lsp
      must_not_grep "$ocfg" '"mode":"subagent"'
      harbor-opencode check --kind alpha --openpencil --root d-op
      expect_die 'plain check must see the openpencil render as stale' \
        harbor-opencode check --kind alpha --root d-op
      harbor-opencode sync --kind alpha --root d-op
      must_not_grep "$ocfg" openpencil-desktop

      # An in-sync sync must skip the write entirely (mtime untouched).
      acfg=d-alpha/.opencode/opencode.jsonc
      touch -d @1000000000 "$acfg"
      before="$(stat -c %Y "$acfg")"
      harbor-opencode sync --kind detect --root d-alpha
      after="$(stat -c %Y "$acfg")"
      if [ "$before" != "$after" ]; then
        echo "sync rewrote an in-sync config ($before -> $after)" >&2
        exit 1
      fi

      # Ownership: the marker must be a top-level key with the exact
      # value. A comment mentioning it, a nested copy, or another value
      # is still hand-written and stays refused without --force.
      mkdir -p d-comment/.opencode d-nested/.opencode d-wrong/.opencode
      printf '%s\n' '{
        // harbor.meta/opencode-config appears only in this comment
        "custom": true
      }' > d-comment/.opencode/opencode.jsonc
      printf '%s\n' '{"nested":{"harbor.meta/opencode-config":"1"},"custom":true}' \
        > d-nested/.opencode/opencode.jsonc
      printf '%s\n' '{"harbor.meta/opencode-config":"0","custom":true}' \
        > d-wrong/.opencode/opencode.jsonc

      for d in d-comment d-nested d-wrong; do
        expect_die "$d must be refused without --force" \
          harbor-opencode sync --kind detect --root "$d"
        must_grep "$d/.opencode/opencode.jsonc" '"custom"'
        harbor-opencode sync --kind detect --force --root "$d"
        must_grep "$d/.opencode/opencode.jsonc" '"alejandra *":"deny"'
        must_grep "$d/.opencode/opencode.jsonc" '"${marker}":"${markerValue}"'
      done

      # JSONC ownership: a marked config with comments is owned (stale, not
      # custom) and syncs without --force; unparsable bytes, an unterminated
      # block comment, comments joining split tokens, and a marker that
      # appears only as a string value stay custom and refused. Comments are
      # whitespace, never token joiners.
      mkdir -p d-jsonc/.opencode d-broken/.opencode d-strval/.opencode d-unclosed/.opencode d-split-true/.opencode d-split-num/.opencode d-quoted/.opencode
      printf '%s\n' '{
        // owned config with a comment: marker is a real top-level key
        /* block comment */ "${marker}": "${markerValue}",
        "custom": true
      }' > d-jsonc/.opencode/opencode.jsonc
      printf '%s\n' '{ not json at all "custom": true' > d-broken/.opencode/opencode.jsonc
      printf '%s\n' '{"note": "${marker}", "custom": true}' > d-strval/.opencode/opencode.jsonc
      printf '%s\n' '{"${marker}":"${markerValue}","custom":true}' '/* unterminated comment' > d-unclosed/.opencode/opencode.jsonc
      printf '%s\n' '{"${marker}":"${markerValue}","custom":tru/**/e}' > d-split-true/.opencode/opencode.jsonc
      printf '%s\n' '{"${marker}":"${markerValue}","custom":true,"n":1/**/2}' > d-split-num/.opencode/opencode.jsonc
      printf '%s\n' '{"${marker}":"${markerValue}","note":"/* not a comment // still not","quote":"a\"b","custom":true}' > d-quoted/.opencode/opencode.jsonc
      expect_die "d-broken check must fail" \
        harbor-opencode check --kind detect --root d-broken
      expect_die "d-unclosed check must fail" \
        harbor-opencode check --kind detect --root d-unclosed
      expect_die "d-split-true check must fail" \
        harbor-opencode check --kind detect --root d-split-true
      expect_die "d-split-num check must fail" \
        harbor-opencode check --kind detect --root d-split-num
      harbor-opencode sync --kind detect --root d-jsonc
      must_grep d-jsonc/.opencode/opencode.jsonc '"${marker}":"${markerValue}"'
      harbor-opencode sync --kind detect --root d-quoted
      must_grep d-quoted/.opencode/opencode.jsonc '"${marker}":"${markerValue}"'
      for d in d-broken d-strval d-unclosed d-split-true d-split-num; do
        cp "$d/.opencode/opencode.jsonc" "$d.before"
        expect_die "$d must be refused without --force" \
          harbor-opencode sync --kind detect --root "$d"
        if ! cmp -s "$d.before" "$d/.opencode/opencode.jsonc"; then
          echo "$d was modified by refused sync" >&2
          exit 1
        fi
        must_grep "$d/.opencode/opencode.jsonc" '"custom"'
        harbor-opencode sync --kind detect --force --root "$d"
        must_grep "$d/.opencode/opencode.jsonc" '"${marker}":"${markerValue}"'
      done

      # A stale-but-marked render (an older harbor config) is refreshed
      # without --force.
      harbor-opencode sync --kind alpha --root d-alpha
      sed 's/fixture-alpha-lsp/fixture-alpha-lsp-OLD/' "$acfg" > "$acfg.rewrite"
      mv -f "$acfg.rewrite" "$acfg"
      must_grep "$acfg" fixture-alpha-lsp-OLD
      harbor-opencode sync --kind detect --root d-alpha
      must_not_grep "$acfg" fixture-alpha-lsp-OLD

      mkdir -p "$out"
      echo ok > "$out/result"
    '';

    # The harbor-meta package deliberately binds no profiles: `detect`
    # resolves to the policy-only config and every language-aware kind is
    # rejected. Language-aware builds ship their own mkCli from harbor-rs.
    harbor-opencode-profile-less = pkgs.runCommand "harbor-meta-harbor-opencode-profile-less" {} ''
      set -e
      ${helpers}
      export PATH=${harborOpencode}/bin:$PATH

      mkdir -p proj
      harbor-opencode sync --kind detect --root proj
      cfg=proj/.opencode/opencode.jsonc
      must_grep "$cfg" '"${marker}":"${markerValue}"'
      must_grep "$cfg" '"alejandra *":"deny"'
      must_not_grep "$cfg" '"lsp"'
      harbor-opencode check --kind detect --root proj
      expect_detect none proj

      expect_die 'profile-less build must reject --kind rust' \
        harbor-opencode sync --kind rust --root proj
      expect_die 'profile-less build must reject --kind python' \
        harbor-opencode sync --kind python --root proj

      harbor-opencode --help > help.txt
      must_grep help.txt 'detect|none'
      must_not_grep help.txt rust

      mkdir -p "$out"
      echo ok > "$out/result"
    '';

    package-tests-plan-shape = let
      builder = lib.packageTests.mkArtifactBuilder {
        kind = "chocolatey-builder";
        packageName = "demo";
        version = "1.0.0";
        output = "${pkgs.emptyDirectory}/demo.1.0.0.nupkg";
      };
      p = lib.packageTests.mkPlan {
        kind = "generic";
        packageName = "demo";
        version = "1.0.0";
        artifacts = [
          {
            name = "demo.tar.gz";
            path = "${pkgs.emptyDirectory}/demo.tar.gz";
          }
        ];
        install.command = "install demo";
        verify = [
          {command = "demo --version";}
        ];
      };
      w = lib.packageTests.mkWindowsPlan {
        packageName = "demo";
        version = "1.0.0";
        artifacts = [
          {
            name = "demo.zip";
            path = "${pkgs.emptyDirectory}/demo.zip";
          }
        ];
        installPowerShell = "Write-Host install";
        verifyPowerShell = ["Write-Host verify"];
      };
      c = lib.packageTests.mkChocolateyVagrantPlan {
        packageName = "demo";
        version = "1.0.0";
        nupkg = builder.output;
        inherit builder;
      };
    in
      assert p.hierarchy == ["generic" "generic"];
      assert w.hierarchy == ["generic" "windows" "windows"];
      assert c.builderRef == builder.ref;
      assert c.hierarchy == ["generic-builder" "windows-builder" "chocolatey-builder" "generic" "windows" "chocolatey-vagrant"];
        pkgs.runCommand "harbor-meta-package-tests-plan-shape" {} ''
          mkdir -p $out
          echo ok > $out/result
        '';

    package-tests-validation = let
      badBuilder = builtins.tryEval (lib.packageTests.mkArtifactBuilder {
        kind = "bad-builder";
        packageName = "demo";
        version = "1.0.0";
        output = "${pkgs.emptyDirectory}/demo";
      });
      badRunner = builtins.tryEval (lib.packageTests.mkRunnerBuilder {
        kind = "bad-runner-builder";
        packageName = "demo";
        plan = {};
        runner = "";
      });
      badKind = builtins.tryEval (lib.packageTests.mkPlan {
        kind = "bogus";
        packageName = "demo";
        version = "1.0.0";
        artifacts = [
          {
            name = "demo.tar.gz";
            path = "${pkgs.emptyDirectory}/demo.tar.gz";
          }
        ];
        install.command = "install demo";
      });
      badChoco = builtins.tryEval (lib.packageTests.mkChocolateyVagrantPlan {
        packageName = "Bad_Name";
        version = "1.0.0";
        nupkg = "${pkgs.emptyDirectory}/Bad_Name.1.0.0.nupkg";
      });
      badSource = builtins.tryEval (lib.packageTests.mkChocolateyVagrantPlan {
        packageName = "demo";
        version = "1.0.0";
        nupkg = "${pkgs.emptyDirectory}/demo.1.0.0.nupkg";
        source = "/packages";
      });
    in
      assert !badBuilder.success;
      assert !badRunner.success;
      assert !badKind.success;
      assert !badChoco.success;
      assert !badSource.success;
        pkgs.runCommand "harbor-meta-package-tests-validation" {} ''
          mkdir -p $out
          echo ok > $out/result
        '';

    package-tests-chocolatey-vagrant-render = let
      builder = lib.packageTests.mkArtifactBuilder {
        kind = "chocolatey-builder";
        packageName = "demo";
        version = "1.0.0";
        output = "${pkgs.emptyDirectory}/demo.1.0.0.nupkg";
      };
      plan = lib.packageTests.mkChocolateyVagrantPlan {
        packageName = "demo";
        version = "1.0.0";
        nupkg = builder.output;
        inherit builder;
        verifyPowerShell = ["demo --version"];
      };
      rendered = lib.packageTests.mkPackageTestRunner {inherit pkgs plan;};
      vagrantfile = lib.packageTests.renderVagrantfile plan;
      bundle = lib.packageTests.mkBuildTestBundle {
        artifactBuilder = builder;
        inherit plan;
        inherit (rendered) runnerBuilder;
      };
    in
      assert pkgs.lib.hasInfix "chocolatey/test-environment" vagrantfile;
      assert pkgs.lib.hasInfix ''config.vm.synced_folder "packages", "/packages"'' vagrantfile;
      assert pkgs.lib.hasInfix ''v.customize ["modifyvm", :id, "--memory", "6144"]'' vagrantfile;
      assert pkgs.lib.hasInfix ''v.customize ["modifyvm", :id, "--cpus", "4"]'' vagrantfile;
      assert pkgs.lib.hasInfix "choco install demo --version 1.0.0 --source C:\\packages" vagrantfile;
      assert rendered.runnerBuilder.kind == "chocolatey-vagrant-runner-builder";
      assert bundle.hierarchy == ["generic-builder" "windows-builder" "chocolatey-builder" "generic" "windows" "chocolatey-vagrant" "generic-runner-builder" "chocolatey-vagrant-runner-builder"];
        pkgs.runCommand "harbor-meta-package-tests-chocolatey-vagrant-render" {} ''
          test -f ${rendered.planJson}
          test -x ${rendered.runner}/bin/package-test-demo
          grep -q '"kind":"chocolatey-vagrant"' ${rendered.planJson}
          grep -q '"builderRef":"chocolatey-builder:demo:1.0.0"' ${rendered.planJson}
          grep -q 'vagrant up --provider=virtualbox' ${rendered.runner}/bin/package-test-demo
          mkdir -p $out
          echo ok > $out/result
        '';

    derivation-manifest-contract = assert derivationManifest.schemaVersion == 1;
    assert derivationManifest.system == pkgs.stdenv.hostPlatform.system;
    assert derivationManifest.targets.default.outputs == ["out"];
    assert derivationManifest.targets.dev.outputs == ["dev"];
    assert derivationManifest.targets.multiple.outputs == ["dev" "out"];
    assert builtins.getContext (builtins.toJSON derivationManifest) == {};
    assert !staticTarget.success;
    assert !ambiguousTarget.success;
      pkgs.runCommand "harbor-meta-derivation-manifest-contract" {} ''
        touch "$out"
      '';

    # Fail if flake inputs ever point at retired forge mirrors again
    # (fleet migrated to github.com/caniko/*). sourceUrl package metadata
    # is informational only, never fetched, so it is excluded.
    site-host-pinning =
      pkgs.runCommand "harbor-meta-site-host-pinning" {
        rootFlakeNix = ../flake.nix;
        rootFlakeLock = ../flake.lock;
        siteFlakeNix = ../site/flake.nix;
        siteFlakeLock = ../site/flake.lock;
      } ''
        if ${pkgs.gnugrep}/bin/grep -v sourceUrl "$rootFlakeNix" "$rootFlakeLock" "$siteFlakeNix" "$siteFlakeLock" \
          | ${pkgs.gnugrep}/bin/grep -E -q "codeberg|codefloe"; then
          echo "ERROR: retired forge host in flake inputs:" >&2
          ${pkgs.gnugrep}/bin/grep -v sourceUrl "$rootFlakeNix" "$rootFlakeLock" "$siteFlakeNix" "$siteFlakeLock" \
            | ${pkgs.gnugrep}/bin/grep -E -n "codeberg|codefloe" >&2 || true
          exit 1
        fi
        touch "$out"
      '';

    format-policy-contract = let
      fp = lib.opencode.formatPolicy;
      match = fp.matcher.match;
      docs = builtins.readFile ../docs/treefmt.md;
      raw = pkgs.lib.concatMap (entry: entry.patterns) fp.registry;
      firstToken = pattern: builtins.head (pkgs.lib.splitString " " pattern);
      policyOnly = builtins.toJSON (lib.opencode.mkConfig {});
      defaultedShape = builtins.toJSON (lib.opencode.mkConfig {
        lsp = {};
        openpencil = false;
      });
      withExtra = builtins.toJSON (lib.opencode.mkConfig {extraFormatDenies = ["myfmt *"];});
      withoutPolicy = builtins.toJSON (lib.opencode.mkConfig {formatPermissions = false;});
      lspJson = builtins.toJSON (lib.opencode.mkConfig {lsp = fixtureLsp;});

      # Registry contract probes: each malformed input must fail closed
      # (assertions and throws are caught under deepSeq + tryEval; shallow
      # tryEval alone would miss the lazy per-profile thunks).
      badName = builtins.tryEval (builtins.deepSeq (lib.opencode.mkRegistry {Alpha = {};}) true);
      reservedName = builtins.tryEval (builtins.deepSeq (lib.opencode.mkRegistry {detect = {};}) true);
      unknownKey = builtins.tryEval (builtins.deepSeq (lib.opencode.mkRegistry {
          alpha = {
            lsp = {};
            package = [];
          };
        })
        true);
      nonListDetect = builtins.tryEval (builtins.deepSeq (lib.opencode.mkRegistry {
          alpha = {
            lsp = {};
            detect.files = "ALPHA";
          };
        })
        true);
      missingLsp = builtins.tryEval (builtins.deepSeq (lib.opencode.mkRegistry {alpha = {};}) true);
      tooManyProfiles = builtins.tryEval (builtins.deepSeq (lib.opencode.profileSubsets ["a" "b" "c" "d" "e" "f" "g"]) true);
      validRegistry = builtins.deepSeq fixtureRegistry true;
      subsets = lib.opencode.profileSubsets ["alpha" "beta"];
    in
      # Registry shape: every base pattern starts with a literal command name.
      # A broad glob like `*fmt *` (or a wildcard-bearing first token) would
      # deny unrelated commands; the env/store twins are generated, never
      # registered by hand.
      assert builtins.all (pattern: !(pkgs.lib.hasPrefix "*" pattern)) raw;
      assert builtins.all (pattern: !(pkgs.lib.hasInfix "*" (firstToken pattern))) raw;
      assert builtins.length raw == builtins.length (pkgs.lib.unique raw);
      # Doc coverage: every registry pattern appears verbatim in
      # docs/treefmt.md's Agent Formatter Policy table.
      assert builtins.all (pattern: pkgs.lib.hasInfix pattern docs) fp.patterns;
      # Glob -> POSIX replay of the deployed matcher semantics
      # (caniko/opencode@f18083c78e packages/core/src/util/wildcard.ts:
      # escape ERE metachars, `*` -> `.*`, trailing ` .*` -> `( .*)?`,
      # anchored full match).
      assert match "alejandra *" "alejandra --check .";
      assert match "alejandra *" "alejandra";
      assert !(match "alejandra *" "alejandrafmt --check .");
      assert !(match "alejandra *" "FOO=1 alejandra --check .");
      assert match "*=* alejandra *" "FOO=1 alejandra --check .";
      assert !(match "*=* alejandra *" "alejandra --check .");
      assert match "/nix/store/*/bin/alejandra *" "/nix/store/abc123-alejandra/bin/alejandra --check .";
      assert !(match "/nix/store/*/bin/alejandra *" "./result/bin/alejandra --check .");
      assert match "./result/bin/alejandra *" "./result/bin/alejandra --check .";
      assert match "cargo fmt *" "cargo fmt";
      assert !(match "cargo fmt *" "cargo build --release");
      assert match "shfmt -d *" "shfmt -d ./x.sh";
      assert match "ruff check *--fix*" "ruff check --fix .";
      assert match "ruff check *--fix*" "ruff check src --fix";
      assert !(match "ruff check *--fix*" "ruff check .");
      assert !(match "prettier *" "prettierx --write .");
      assert match "nix fmt *" "nix fmt";
      assert match "just --fmt *" "just --fmt --unstable";
      assert match "go fmt *" "go fmt ./...";
      # statix runs as the statix-fix wrapper binary: `statix *` does not
      # cover it (no space after `statix`), so the registry owns both.
      assert match "statix-fix *" "statix-fix";
      assert !(match "statix *" "statix-fix");
      assert match "/nix/store/*/bin/statix-fix *" "/nix/store/abc-statix-fix/bin/statix-fix";
      # Rendered shapes: policy-only has no lsp block and carries the deny
      # fragment + identity marker; explicit defaults render identically
      # to the implicit ones; profile lsp blocks ride along; no allow
      # values and no catch-all ever appear; extras get twin expansion;
      # the identity marker survives even with formatPermissions = false.
      assert !(pkgs.lib.hasInfix "\"lsp\"" policyOnly);
      assert policyOnly == defaultedShape;
      assert pkgs.lib.hasInfix "\"permission\"" policyOnly;
      assert pkgs.lib.hasInfix "\"${marker}\":\"${markerValue}\"" policyOnly;
      assert pkgs.lib.hasInfix "\"alejandra *\":\"deny\"" policyOnly;
      assert pkgs.lib.hasInfix "\"*=* alejandra *\":\"deny\"" policyOnly;
      assert pkgs.lib.hasInfix "\"/nix/store/*/bin/alejandra *\":\"deny\"" policyOnly;
      assert !(pkgs.lib.hasInfix ":\"allow\"" policyOnly);
      assert !(pkgs.lib.hasInfix "\"*\":\"deny\"" policyOnly);
      assert pkgs.lib.hasInfix "\"lsp\"" lspJson;
      assert pkgs.lib.hasInfix "\"fixture-alpha-lsp\"" lspJson;
      assert pkgs.lib.hasInfix "\"myfmt *\":\"deny\"" withExtra;
      assert pkgs.lib.hasInfix "\"*=* myfmt *\":\"deny\"" withExtra;
      assert !(pkgs.lib.hasInfix "\"permission\"" withoutPolicy);
      assert pkgs.lib.hasInfix "\"${marker}\":\"${markerValue}\"" withoutPolicy;
      # The kind-based config API is gone: `kind` is not a parameter, so
      # passing it fails closed at the call site.
      assert !(builtins.functionArgs lib.opencode.mkConfig ? kind);
      # Profile registry: malformed names, reserved keywords, unknown keys,
      # malformed detect lists, and missing lsp blocks all fail closed; the
      # valid fixture registry evaluates fully.
      assert !badName.success;
      assert !reservedName.success;
      assert !unknownKey.success;
      assert !nonListDetect.success;
      assert !missingLsp.success;
      assert validRegistry;
      # Subset enumeration is deterministic, registry-ordered, and capped.
      assert subsets == [[] ["alpha"] ["beta"] ["alpha" "beta"]];
      assert !tooManyProfiles.success;
      assert lib.opencode.profileKey [] == "none";
      assert lib.opencode.profileKey ["alpha" "beta"] == "alpha,beta";
      # lspFor merges exactly the requested subset in registry order.
      assert builtins.attrNames (lib.opencode.lspFor fixtureRegistry ["alpha"]) == ["fixture-alpha-lsp"];
      assert builtins.attrNames (lib.opencode.lspFor fixtureRegistry ["alpha" "beta"]) == ["fixture-alpha-lsp" "fixture-beta-lsp"];
        pkgs.runCommand "harbor-meta-format-policy-contract" {} ''
          mkdir -p "$out"
          echo ok > "$out/result"
        '';

    format-policy-render = let
      fp = lib.opencode.formatPolicy;
      denyGreps =
        pkgs.lib.concatMapStringsSep "\n" (pattern: ''
          grep -qF '"${pattern}":"deny"' lsp.json
          grep -qF '"${pattern}":"deny"' none.json
          grep -qF '"*=* ${pattern}":"deny"' lsp.json
        '')
        fp.patterns;
    in
      pkgs.runCommand "harbor-meta-format-policy-render" {
        lspConfig = lib.opencode.configTextFor {lsp = fixtureLsp;};
        noneConfig = lib.opencode.configTextFor {};
        defaultedConfig = lib.opencode.configTextFor {
          lsp = {};
          openpencil = false;
        };
        fixtureOpenpencilConfig = lib.opencode.configTextFor {
          lsp = fixtureLsp;
          openpencil = true;
        };
        passAsFile = [
          "lspConfig"
          "noneConfig"
          "defaultedConfig"
          "fixtureOpenpencilConfig"
        ];
      } ''
        set -e
        ${helpers}

        cp "$lspConfigPath" lsp.json
        cp "$noneConfigPath" none.json
        cp "$defaultedConfigPath" defaulted.json
        cp "$fixtureOpenpencilConfigPath" fixture-openpencil.json

        ${denyGreps}

        # Identity marker in every rendered shape (used by sync/refusal).
        must_grep lsp.json '"${marker}":"${markerValue}"'
        must_grep none.json '"${marker}":"${markerValue}"'
        must_grep defaulted.json '"${marker}":"${markerValue}"'
        must_grep fixture-openpencil.json '"${marker}":"${markerValue}"'

        # Deny-only: no allow anywhere, no catch-all key.
        must_not_grep lsp.json ':"allow"'
        must_not_grep none.json ':"allow"'
        must_not_grep fixture-openpencil.json ':"allow"'
        must_not_grep lsp.json '"*":"deny"'
        must_not_grep lsp.json '"*":"ask"'

        # Policy-only shapes carry no lsp block; profile shapes keep theirs.
        must_not_grep none.json '"lsp"'
        must_not_grep defaulted.json '"lsp"'
        must_grep lsp.json '"lsp"'
        must_grep lsp.json fixture-alpha-lsp

        # Openpencil and the format policy coexist in one document.
        must_grep fixture-openpencil.json openpencil-desktop
        must_grep fixture-openpencil.json '"alejandra *":"deny"'
        must_not_grep fixture-openpencil.json '"mode":"subagent"'

        mkdir -p "$out"
        echo ok > "$out/result"
      '';

    harbor-opencode-rollout-check = pkgs.runCommand "harbor-meta-opencode-rollout-check" {} ''
      set -e
      ${helpers}
      export PATH=${fixtureCli}/bin:${pkgs.git}/bin:$PATH

      # Hermetic fixture git: the runCommand sandbox carries no operator
      # git config, and these exports keep it that way (identity comes
      # from disposable-fixture -c flags; no global ignore can hide
      # .opencode/ from the cleanliness scan).
      mkdir -p xdg-empty
      export XDG_CONFIG_HOME="$PWD/xdg-empty"
      export GIT_CONFIG_GLOBAL=/dev/null
      g() {
        git -c core.excludesFile=/dev/null -c user.name='Harbor CI' \
          -c user.email=ci@example.com "$@"
      }

      for name in alphaproj betaproj plainproj customproj dirtyproj; do
        mkdir -p "fleet/$name"
        g -C "fleet/$name" init -q
      done
      touch fleet/alphaproj/ALPHA
      touch fleet/betaproj/BETA
      # plainproj needs committed content: a bare `git commit` in an empty
      # repository exits 1, which would silently poison every summary.
      printf 'readme\n' > fleet/plainproj/README.md
      mkdir -p fleet/customproj/.opencode
      printf '%s\n' '{"$schema":"https://opencode.ai/config.json","custom":true}' \
        > fleet/customproj/.opencode/opencode.jsonc
      printf 'another session is here\n' > fleet/dirtyproj/unrelated-change.txt
      for name in alphaproj betaproj plainproj customproj; do
        g -C "fleet/$name" add -A
        g -C "fleet/$name" commit -qm init
      done

      # Run 1: writes clean repos, refuses the hand-written config, blocks
      # the dirty tree — an exit 0 here would hide all three.
      if harbor-opencode rollout --root fleet > run1.log 2>&1; then
        echo "rollout exited 0 despite a custom config and a dirty repo" >&2
        cat run1.log >&2
        exit 1
      fi
      must_grep run1.log 'total=5 synced=3 unchanged=0 blocked-dirty=1 custom=1 dirty=1'
      must_grep run1.log 'blocked (dirty working tree)'
      must_grep fleet/alphaproj/.opencode/opencode.jsonc fixture-alpha-lsp
      must_grep fleet/betaproj/.opencode/opencode.jsonc fixture-beta-lsp
      must_grep fleet/plainproj/.opencode/opencode.jsonc '"alejandra *":"deny"'
      must_not_grep fleet/plainproj/.opencode/opencode.jsonc '"lsp"'
      must_grep fleet/customproj/.opencode/opencode.jsonc '"custom":true'
      must_not_grep fleet/customproj/.opencode/opencode.jsonc "${marker}"
      if [ -f fleet/dirtyproj/.opencode/opencode.jsonc ]; then
        echo "dirty repo was written during rollout" >&2
        exit 1
      fi

      # Read-only check reports the same state (dirtyproj is still missing
      # its config).
      if harbor-opencode rollout --root fleet --check > run2.log 2>&1; then
        echo "rollout --check exited 0 with a custom config and a missing config" >&2
        cat run2.log >&2
        exit 1
      fi
      must_grep run2.log 'total=5 ok=3 stale=0 missing=1 custom=1 dirty=1'

      # Run 2: --force replaces the custom config; the dirty tree still
      # blocks, and already-synced repos are untouched.
      if harbor-opencode rollout --root fleet --force > run3.log 2>&1; then
        echo "rollout --force exited 0 while a repo still has foreign dirt" >&2
        cat run3.log >&2
        exit 1
      fi
      must_grep run3.log 'total=5 synced=1 unchanged=3 blocked-dirty=1 custom=0 dirty=1'
      must_grep fleet/customproj/.opencode/opencode.jsonc '"alejandra *":"deny"'
      must_grep fleet/customproj/.opencode/opencode.jsonc '"${marker}":"${markerValue}"'
      if [ -f fleet/dirtyproj/.opencode/opencode.jsonc ]; then
        echo "dirty repo was written during forced rollout" >&2
        exit 1
      fi

      # Run 3: once the foreign file is gone the repo is picked up again,
      # and a repeated rollout is not blocked by its own previous writes.
      # Without --untracked-files=all the untracked .opencode/ from runs
      # 1-2 collapses to `?? .opencode/` and would read as foreign dirt;
      # our write-temp namespace must be tolerated too.
      rm fleet/dirtyproj/unrelated-change.txt
      printf 'leftover\n' > fleet/betaproj/.opencode/opencode.jsonc.tmp.stray
      if harbor-opencode rollout --root fleet > run4.log 2>&1; then :; else
        echo "repeat rollout must succeed once the foreign file is gone" >&2
        cat run4.log >&2
        exit 1
      fi
      must_grep run4.log 'total=5 synced=1 unchanged=4 blocked-dirty=0 custom=0 dirty=0'
      if [ ! -f fleet/betaproj/.opencode/opencode.jsonc.tmp.stray ]; then
        echo "a stray write-temp was deleted (allowed dirt must be left alone)" >&2
        exit 1
      fi

      # Read-only verification passes once every config is rendered.
      if harbor-opencode rollout --root fleet --check > run5.log 2>&1; then :; else
        echo "final rollout --check must pass" >&2
        cat run5.log >&2
        exit 1
      fi
      must_grep run5.log 'total=5 ok=5 stale=0 missing=0 custom=0 dirty=0'

      # Malformed JSONC stays custom through rollout and is preserved
      # byte-for-byte in clean repos (dirty-tree rejection cannot be the
      # reason preservation passes here). Cover all three split-token
      # shapes from sync-check: tru/**/e, 1/**/2, and unterminated comment.
      mkdir -p fleet2/malformedproj fleet2/malformednumproj fleet2/malformedunclosedproj
      g -C fleet2/malformedproj init -q
      g -C fleet2/malformednumproj init -q
      g -C fleet2/malformedunclosedproj init -q
      mkdir -p fleet2/malformedproj/.opencode fleet2/malformednumproj/.opencode fleet2/malformedunclosedproj/.opencode
      printf '%s\n' '{"${marker}":"${markerValue}","custom":tru/**/e}' > fleet2/malformedproj/.opencode/opencode.jsonc
      printf '%s\n' '{"${marker}":"${markerValue}","custom":true,"n":1/**/2}' > fleet2/malformednumproj/.opencode/opencode.jsonc
      printf '%s\n' '{"${marker}":"${markerValue}","custom":true}' '/* unterminated comment' > fleet2/malformedunclosedproj/.opencode/opencode.jsonc
      cp fleet2/malformedproj/.opencode/opencode.jsonc fleet2.before-split-true
      cp fleet2/malformednumproj/.opencode/opencode.jsonc fleet2.before-split-num
      cp fleet2/malformedunclosedproj/.opencode/opencode.jsonc fleet2.before-unclosed
      g -C fleet2/malformedproj add -A
      g -C fleet2/malformedproj commit -qm init
      g -C fleet2/malformednumproj add -A
      g -C fleet2/malformednumproj commit -qm init
      g -C fleet2/malformedunclosedproj add -A
      g -C fleet2/malformedunclosedproj commit -qm init
      if harbor-opencode rollout --root fleet2 --check > run-malformed-check.log 2>&1; then
        echo "malformed rollout --check exited 0" >&2
        cat run-malformed-check.log >&2
        exit 1
      fi
      must_grep run-malformed-check.log 'total=3 ok=0 stale=0 missing=0 custom=3 dirty=0'
      if ! cmp -s fleet2.before-split-true fleet2/malformedproj/.opencode/opencode.jsonc; then
        echo "malformed split-true config changed by rollout --check" >&2
        exit 1
      fi
      if ! cmp -s fleet2.before-split-num fleet2/malformednumproj/.opencode/opencode.jsonc; then
        echo "malformed split-num config changed by rollout --check" >&2
        exit 1
      fi
      if ! cmp -s fleet2.before-unclosed fleet2/malformedunclosedproj/.opencode/opencode.jsonc; then
        echo "malformed unclosed config changed by rollout --check" >&2
        exit 1
      fi
      if harbor-opencode rollout --root fleet2 > run-malformed.log 2>&1; then
        echo "malformed rollout exited 0" >&2
        cat run-malformed.log >&2
        exit 1
      fi
      must_grep run-malformed.log 'custom=3'
      if ! cmp -s fleet2.before-split-true fleet2/malformedproj/.opencode/opencode.jsonc; then
        echo "malformed split-true config changed by refused rollout" >&2
        exit 1
      fi
      if ! cmp -s fleet2.before-split-num fleet2/malformednumproj/.opencode/opencode.jsonc; then
        echo "malformed split-num config changed by refused rollout" >&2
        exit 1
      fi
      if ! cmp -s fleet2.before-unclosed fleet2/malformedunclosedproj/.opencode/opencode.jsonc; then
        echo "malformed unclosed config changed by refused rollout" >&2
        exit 1
      fi

      # A git that cannot answer must fail closed as foreign dirt instead
      # of silently treating the tree as clean.
      mkdir -p fleet/brokenproj
      printf 'gitdir: /nonexistent\n' > fleet/brokenproj/.git
      if harbor-opencode rollout --root fleet > run6.log 2>&1; then
        echo "rollout must fail when a repo's git status is unreadable" >&2
        cat run6.log >&2
        exit 1
      fi
      must_grep run6.log 'git status failed'
      must_grep run6.log 'blocked-dirty=1'
      rm -f fleet/brokenproj/.git
      rmdir fleet/brokenproj

      # A scan root without any repository is an error, not a no-op.
      mkdir -p nofleet
      expect_die 'empty scan root' harbor-opencode rollout --root nofleet

      mkdir -p "$out"
      echo ok > "$out/result"
    '';
  }
