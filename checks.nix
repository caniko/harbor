{
  self,
  pkgs,
  system,
  toolchain,
  cross,
  rootInputNames,
  nixpkgs,
  rust-overlay,
  crane,
  treefmt-nix,
  git-hooks,
}: let
  isLinux = builtins.match ".*-linux" system != null;
  rootLock = builtins.fromJSON (builtins.readFile ./flake.lock);
  rootHasPathInput =
    builtins.any
    (nodeName: (rootLock.nodes.${nodeName}.original.type or null) == "path")
    (builtins.filter builtins.isString (builtins.attrValues rootLock.nodes.root.inputs));
  defaultTemplateRoot = ./templates/default;
  defaultTemplateFlake = builtins.readFile (defaultTemplateRoot + "/flake.nix");
  defaultTemplateSimit = builtins.fromTOML (builtins.readFile (defaultTemplateRoot + "/simit.toml"));
  defaultTemplateTreefmt = builtins.readFile (defaultTemplateRoot + "/nix/treefmt.nix");
  defaultTemplateHooks = builtins.readFile (defaultTemplateRoot + "/nix/pre-commit.nix");
  bevyTemplateRoot = ./templates/bevy;
  bevyTemplateFlake = builtins.readFile (bevyTemplateRoot + "/flake.nix");
  bevyTemplateSimit = builtins.fromTOML (builtins.readFile (bevyTemplateRoot + "/simit.toml"));
  bevyTemplateTreefmt = builtins.readFile (bevyTemplateRoot + "/nix/treefmt.nix");
  bevyTemplateHooks = builtins.readFile (bevyTemplateRoot + "/nix/pre-commit.nix");
in
  {
    opencode-rust-default = let
      rustConfig = self.lib.opencode.configTextFor {lsp = self.lib.opencodeRust.rustLsp;};
    in
      assert !(pkgs.lib.hasInfix "openpencil" rustConfig);
      assert pkgs.lib.hasInfix "rust-analyzer" rustConfig;
      assert pkgs.lib.hasInfix "nixd" rustConfig;
      assert pkgs.lib.hasInfix "taplo" rustConfig;
        pkgs.runCommand "check-opencode-rust-default" {} "touch $out";
    mkRustServiceModule-lazy-pkgs = let
      result = self.lib.mkRustServiceModule {
        pkgs = throw "mkRustServiceModule forced pkgs";
        inherit (pkgs) lib;
        name = "rust-service-fixture";
        binary = "/bin/rust-service-fixture";
        args = "--mode smoke";
      };
      service = result.systemd.services.rust-service-fixture;
    in
      assert service.serviceConfig.ExecStart == "/bin/rust-service-fixture --mode smoke";
      assert result.users.users.rust-service-fixture.isSystemUser;
        pkgs.runCommand "check-mkRustServiceModule-lazy-pkgs" {} "touch $out";
    maven-cache-module-shape = let
      evaluated = nixpkgs.lib.nixosSystem {
        inherit system;
        modules = [
          self.nixosModules.mavenCache
          {
            services.harborRs.mavenCache = {
              enable = true;
              mirrors = ["https://upstream.example.test/maven/"];
              storageMount = "/var/cache/reposilite";
            };
          }
        ];
      };
      cfg = evaluated.config;
      cache = cfg.services.harborRs.mavenCache;
      initScript = self.lib.mkGradleMavenProxyInitScript {
        inherit pkgs;
        repositoryUrl = "http://127.0.0.1:8081/cache";
      };
    in
      assert cfg.services.reposilite.enable;
      assert cfg.services.reposilite.database.type == "sqlite";
      assert cfg.services.reposilite.database.path == "reposilite.db";
        pkgs.runCommand "check-maven-cache-module-shape" {
          nativeBuildInputs = [pkgs.gradle pkgs.jq];
        } ''
          jq -e '
            .maven.repositories == [{
              id: "cache",
              metadataMaxAge: 0,
              preserveSnapshots: false,
              proxied: [{
                allowedExtensions: [".jar", ".war", ".pom", ".xml", ".module", ".md5", ".sha1", ".sha256", ".sha512", ".asc", ".aar"],
                allowedGroups: [],
                authenticatedFetchingOnly: false,
                connectTimeout: 3,
                httpProxy: "",
                readTimeout: 15,
                reference: "https://upstream.example.test/maven/",
                store: true
              }],
              redeployment: false,
              storagePolicy: "PRIORITIZE_UPSTREAM_METADATA",
              storageProvider: {
                maxResourceLockLifetimeInSeconds: 60,
                mount: "/var/cache/reposilite",
                quota: "64GB",
                type: "fs"
              },
              visibility: "PUBLIC"
            }]
          ' ${cache.sharedConfiguration}

          mkdir fixture
          cat > fixture/settings.gradle <<'EOF'
          import org.gradle.api.initialization.resolve.RepositoriesMode

          pluginManagement {
            repositories {
              gradlePluginPortal()
              maven { name = "privatePlugin"; url = uri("https://private.example.test/plugins") }
            }
          }
          dependencyResolutionManagement {
            repositoriesMode.set(RepositoriesMode.PREFER_SETTINGS)
            repositories {
              google()
              mavenCentral()
              maven { name = "privateDependency"; url = uri("https://private.example.test/maven") }
            }
          }
          rootProject.name = "maven-cache-fixture"

          gradle.settingsEvaluated { settings ->
            def pluginUrls = settings.pluginManagement.repositories.findAll { it.hasProperty("url") }.collect { it.url.toString() }
            def dependencyUrls = settings.dependencyResolutionManagement.repositories.findAll { it.hasProperty("url") }.collect { it.url.toString() }
            assert pluginUrls.contains("http://127.0.0.1:8081/cache")
            assert pluginUrls.contains("https://private.example.test/plugins")
            assert !pluginUrls.any { it.startsWith("https://plugins.gradle.org") }
            assert dependencyUrls.contains("http://127.0.0.1:8081/cache")
            assert dependencyUrls.contains("https://private.example.test/maven")
            assert !dependencyUrls.any { it.startsWith("https://repo.maven.apache.org") || it.startsWith("https://dl.google.com") }
          }
          EOF
          touch fixture/build.gradle
          export GRADLE_USER_HOME="$TMPDIR/gradle-home"
          gradle --offline --no-daemon --init-script ${initScript} -p fixture help
          touch "$out"
        '';

    maven-cache-proxy-persistence = pkgs.testers.nixosTest {
      name = "maven-cache-proxy-persistence";
      nodes.machine = {pkgs, ...}: {
        imports = [self.nixosModules.mavenCache];

        services.harborRs.mavenCache = {
          enable = true;
          mirrors = ["http://127.0.0.1:9000/"];
        };
        services.reposilite.settings.port = 8081;

        systemd.services.maven-upstream = {
          wantedBy = ["multi-user.target"];
          before = ["reposilite.service"];
          serviceConfig.Type = "exec";
          preStart = ''
            install -d /var/lib/maven-upstream/com/example/demo/1.0
            printf fixture-aar > /var/lib/maven-upstream/com/example/demo/1.0/demo-1.0.aar
          '';
          script = "exec ${pkgs.python3}/bin/python -m http.server 9000 --directory /var/lib/maven-upstream";
        };

        environment.systemPackages = [pkgs.curl];
      };
      testScript = ''
        start_all()
        machine.wait_for_unit("maven-upstream.service")
        machine.wait_for_unit("reposilite.service")
        machine.wait_for_open_port(8081)
        machine.succeed("curl -fsS http://127.0.0.1:8081/cache/com/example/demo/1.0/demo-1.0.aar -o /tmp/first.aar")
        machine.succeed("grep -qx fixture-aar /tmp/first.aar")
        machine.succeed("test -s /var/lib/reposilite/reposilite.db")
        machine.succeed("systemctl stop maven-upstream.service")
        machine.succeed("systemctl restart reposilite.service")
        machine.wait_for_open_port(8081)
        machine.succeed("curl -fsS http://127.0.0.1:8081/cache/com/example/demo/1.0/demo-1.0.aar -o /tmp/second.aar")
        machine.succeed("cmp /tmp/first.aar /tmp/second.aar")
      '';
    };

    mkRustCommandServiceModule-shape = let
      package = pkgs.writeShellScriptBin "rust-service-fixture" "exit 0";
      result = self.lib.mkRustCommandServiceModule {
        inherit pkgs package;
        name = "rust-service-fixture";
        executable = "bin/rust-service-fixture";
        args = ["--mode" "smoke test"];
        user = "fixture-user";
        group = "fixture-group";
        type = "oneshot";
        wantedBy = [];
      };
      service = result.systemd.services.rust-service-fixture;
    in
      assert (service.serviceConfig.Type or "simple") == "oneshot";
      assert service.serviceConfig.User == "fixture-user";
      assert service.serviceConfig.Group == "fixture-group";
      assert pkgs.lib.hasInfix "--mode" service.serviceConfig.ExecStart;
      assert pkgs.lib.hasInfix "smoke test" service.serviceConfig.ExecStart;
        pkgs.runCommand "check-mkRustCommandServiceModule-shape" {} "touch $out";

    binary-release-helper-shape = let
      fixturePackage = pkgs.runCommand "binary-release-fixture" {} ''
        mkdir -p "$out/bin"
        printf '#!/bin/sh\necho fixture\n' > "$out/bin/fixture"
        chmod 0755 "$out/bin/fixture"
      '';
      release = self.lib.mkBinaryRelease {
        inherit pkgs;
        pname = "fixture";
        version = "0.1.0";
        artifacts.x86_64-linux = {
          package = fixturePackage;
          system = "x86_64-linux";
          rustTarget = "x86_64-unknown-linux-musl";
          binaries = ["fixture"];
        };
      };
      consumer = self.lib.mkReleaseBinaryPackage {
        inherit pkgs;
        pname = "fixture";
        version = "0.1.0";
        sources.${system} = fixturePackage;
        binaries = ["fixture"];
      };
    in
      assert release.archives ? "x86_64-linux";
      assert pkgs.lib.isDerivation release.bundle;
      assert pkgs.lib.isDerivation release.releaseBundle;
      assert pkgs.lib.isDerivation consumer;
        pkgs.runCommand "check-binary-release-helper-shape" {} "touch $out";

    binary-release-consumer-rejects-missing-system = let
      result =
        builtins.tryEval
        (self.lib.mkReleaseBinaryPackage {
          inherit pkgs;
          pname = "fixture";
          version = "0.1.0";
          sources = {};
          binaries = ["fixture"];
        }).drvPath;
    in
      assert !result.success;
        pkgs.runCommand "check-binary-release-consumer-rejects-missing-system" {} "touch $out";

    portable-release-helper-shape = let
      fixturePackage = pkgs.runCommand "portable-release-fixture" {} ''
        mkdir -p "$out/bin"
        printf '#!/bin/sh\necho fixture\n' > "$out/bin/fixture"
        chmod 0755 "$out/bin/fixture"
      '';
      fixtureBundle = pkgs.writeScript "portable-fixture-bundle" "#!/bin/sh\necho fixture\n";
      release = self.lib.mkPortableBinaryRelease {
        inherit pkgs;
        pname = "fixture";
        version = "0.1.0";
        artifacts.x86_64-linux = {
          bundler = _: fixtureBundle;
          entries.fixture.package = fixturePackage;
        };
      };
      source = pkgs.runCommand "portable-release-source" {} ''
        mkdir -p "$out/bin"
        printf '#!/bin/sh\necho fixture\n' > "$out/bin/fixture"
        chmod 0755 "$out/bin/fixture"
        cat > "$out/manifest.json" <<'EOF'
        {"schemaVersion":2,"name":"fixture","version":"0.1.0","system":"x86_64-linux","format":"nix-bundle","binaries":["fixture"]}
        EOF
      '';
      consumer = self.lib.mkPortableReleaseBinaryPackage {
        inherit pkgs;
        pname = "fixture";
        version = "0.1.0";
        sources.x86_64-linux = source;
        binaries = ["fixture"];
      };
    in
      assert pkgs.lib.isDerivation release.releaseBundle;
      assert pkgs.lib.isDerivation consumer;
        pkgs.runCommand "check-portable-release-helper-shape" {} "touch $out";

    portable-release-consumer-rejects-missing-system = let
      result =
        builtins.tryEval
        (self.lib.mkPortableReleaseBinaryPackage {
          inherit pkgs;
          pname = "fixture";
          version = "0.1.0";
          sources = {};
          binaries = ["fixture"];
        }).drvPath;
    in
      assert !result.success;
        pkgs.runCommand "check-portable-release-consumer-rejects-missing-system" {} "touch $out";

    portable-release-producer-rejects-empty-binaries = let
      result =
        builtins.tryEval
        (self.lib.mkPortableBinaryRelease {
          inherit pkgs;
          pname = "fixture";
          version = "0.1.0";
          artifacts.x86_64-linux.entries = {};
          artifacts.x86_64-linux.bundler = _: pkgs.writeScript "empty-fixture-bundle" "exit 1";
        }).releaseBundle.drvPath;
    in
      assert !result.success;
        pkgs.runCommand "check-portable-release-producer-rejects-empty-binaries" {} "touch $out";

    generic-release-artifact-contract = let
      source = pkgs.writeText "generic-release-source" "artifact";
      artifact = self.lib.mkReleaseArtifact {
        inherit pkgs source;
        pname = "fixture";
        version = "0.1.0";
        name = "fixture.tar.gz";
        kind = "binary-archive";
        format = "tar.gz";
        system = "x86_64-linux";
        rustTarget = "x86_64-unknown-linux-musl";
        consumable = true;
      };
      bundle = self.lib.mkReleaseBundle {
        inherit pkgs;
        pname = "fixture";
        version = "0.1.0";
        artifacts = {inherit artifact;};
      };
    in
      assert artifact.rsHarborReleaseArtifact.schemaVersion == 2;
      assert artifact.rsHarborReleaseArtifact.consumable;
      assert bundle.rsHarborReleaseBundle;
        pkgs.runCommand "check-generic-release-artifact-contract" {} "touch $out";

    release-bundle-dual-system-contract = let
      x86Source = pkgs.writeScript "fixture-x86_64" "#!/bin/sh\necho x86_64\n";
      armSource = pkgs.writeScript "fixture-aarch64" "#!/bin/sh\necho aarch64\n";
      x86 = self.lib.mkReleaseArtifact {
        inherit pkgs;
        pname = "fixture";
        version = "0.1.0";
        name = "fixture-0.1.0-x86_64-linux";
        source = x86Source;
        system = "x86_64-linux";
        rustTarget = "x86_64-unknown-linux-musl";
        kind = "binary";
        executable = true;
        consumable = true;
      };
      arm = self.lib.mkReleaseArtifact {
        inherit pkgs;
        pname = "fixture";
        version = "0.1.0";
        name = "fixture-0.1.0-aarch64-linux";
        source = armSource;
        system = "aarch64-linux";
        rustTarget = "aarch64-unknown-linux-musl";
        kind = "binary";
        executable = true;
        consumable = true;
      };
      bundle = self.lib.mkReleaseBundle {
        inherit pkgs;
        pname = "fixture";
        version = "0.1.0";
        artifacts = {inherit x86 arm;};
      };
    in
      pkgs.runCommand "check-release-bundle-dual-system-contract" {
        nativeBuildInputs = [pkgs.jq];
      } ''
        test -x ${bundle}/fixture-0.1.0-x86_64-linux
        test -x ${bundle}/fixture-0.1.0-aarch64-linux
        jq -e '
          (.schemaVersion == 2) and
          ([.artifacts[].system] | sort == ["aarch64-linux", "x86_64-linux"]) and
          ([.artifacts[].name] | sort == ["fixture-0.1.0-aarch64-linux", "fixture-0.1.0-x86_64-linux"])
        ' ${bundle}/fixture-0.1.0-release-manifest.json >/dev/null
        touch "$out"
      '';

    release-bundle-rejects-name-collisions = let
      source = pkgs.writeText "collision-source" "artifact";
      one = self.lib.mkReleaseArtifact {
        inherit pkgs source;
        pname = "fixture";
        version = "0.1.0";
        name = "same-name";
      };
      two = self.lib.mkReleaseArtifact {
        inherit pkgs source;
        pname = "fixture";
        version = "0.1.0";
        name = "same-name";
      };
      result =
        builtins.tryEval
        (self.lib.mkReleaseBundle {
          inherit pkgs;
          pname = "fixture";
          version = "0.1.0";
          artifacts = {inherit one two;};
        }).drvPath;
    in
      assert !result.success;
        pkgs.runCommand "check-release-bundle-rejects-name-collisions" {} "touch $out";

    portable-release-bundle-contract = let
      fixturePackage = pkgs.writeShellScriptBin "portable-fixture" "echo fixture";
      fixtureBundler = _: pkgs.writeScript "portable-fixture-bundle" "#!/bin/sh\necho fixture\n";
      release = self.lib.mkPortableBinaryRelease {
        inherit pkgs;
        pname = "portable-fixture";
        version = "0.1.0";
        artifacts.x86_64-linux = {
          bundler = fixtureBundler;
          entries.portable-fixture.package = fixturePackage;
        };
      };
    in
      pkgs.runCommand "check-portable-release-bundle-contract" {
        nativeBuildInputs = [pkgs.gnutar pkgs.gzip pkgs.jq];
      } ''
        archive=${release.releaseBundle}/portable-fixture-0.1.0-x86_64-linux-nix-bundle.tar.gz
        manifest=${release.releaseBundle}/portable-fixture-0.1.0-release-manifest.json
        test -f "$archive"
        tar -tzf "$archive" | grep -Fx './bin/portable-fixture'
        jq -e '
          (.schemaVersion == 2) and
          (.artifacts | length == 1) and
          (.artifacts[0].kind == "portable-binary-archive") and
          (.artifacts[0].system == "x86_64-linux")
        ' "$manifest" >/dev/null
        touch "$out"
      '';

    # The reusable library flake must not acquire a consumer-site input. The
    # optional Pages publisher lives in ./site, keeping the dependency graph
    # one-way when Plinth consumes harbor-rs. Root path inputs are equally
    # non-portable: a downstream lock cannot resolve them inside this source.
    harbor-rs-root-inputs-no-consumer-site = assert !(builtins.elem "plinth" rootInputNames);
    assert !rootHasPathInput;
      pkgs.runCommand "check-harbor-rs-root-inputs-no-consumer-site" {} "touch $out";

    # mkToolchain returns expected attributes
    mkToolchain-shape = let
      t = self.lib.mkToolchain {inherit pkgs;};
    in
      assert t ? rustToolchain;
      assert t ? craneLib;
      assert t ? rawCraneLib;
      assert t ? buildCache;
      assert t.buildCache == null;
      assert t.craneLib.rsHarborBuildCachePolicy == null;
      assert t.cargoConfig ? configText;
      assert !(pkgs.lib.hasInfix "codegen-backend" t.cargoConfig.configText);
      assert t.craneLib.rsHarborCargoConfig == t.cargoConfig;
      assert t ? crossTargets;
      assert builtins.isList t.crossTargets;
      assert builtins.length t.crossTargets > 0;
        pkgs.runCommand "check-mkToolchain-shape" {} "touch $out";

    # The cache policy is applied at Crane's derivation-construction boundary,
    # so ordinary and dependency-only builds cannot silently bypass it.
    mkToolchain-cache-opt-in = let
      t = self.lib.mkToolchain {
        inherit pkgs;
        cache.enable = true;
      };
      src = t.craneLib.cleanCargoSource ./templates/default;
      deps = t.craneLib.buildDepsOnly {
        inherit src;
        pname = "mk-toolchain-cache-fixture";
        version = "0.1.0";
        rsHarborCacheReuseKey = "mk-toolchain-cache-fixture";
        doCheck = false;
      };
      package = t.craneLib.buildPackage {
        inherit src;
        pname = "mk-toolchain-cache-fixture";
        version = "0.1.0";
        cargoArtifacts = deps;
        rsHarborCacheReuseKey = "mk-toolchain-cache-fixture";
        doCheck = false;
      };
    in
      assert deps.passthru.rsHarborBuildCacheWrapped;
      assert package.passthru.rsHarborBuildCacheWrapped;
      assert deps.drvAttrs.RS_HARBOR_SCCACHE_WORKLOAD_KIND == "dependency-artifacts";
      assert deps.drvAttrs.RS_HARBOR_SCCACHE_REUSE_KEY == "mk-toolchain-cache-fixture";
      assert package.drvAttrs.RS_HARBOR_SCCACHE_WORKLOAD_KIND == "package";
      assert package.drvAttrs.RS_HARBOR_SCCACHE_REUSE_KEY == "mk-toolchain-cache-fixture";
      assert deps.drvAttrs.RS_HARBOR_SCCACHE_COMPILER == t.rustToolchain.version;
      assert package.drvAttrs.RS_HARBOR_SCCACHE_COMPILER == t.rustToolchain.version;
        pkgs.runCommand "check-mkToolchain-cache-opt-in" {} "touch $out";

    # Ordinary consumers remain portable when no host cache transport exists.
    mkToolchain-cache-default-off = let
      t = self.lib.mkToolchain {inherit pkgs;};
      package = t.craneLib.buildPackage {
        src = ./templates/default;
        pname = "mk-toolchain-cache-default-off";
        version = "0.1.0";
        doCheck = false;
      };
    in
      assert t.buildCache == null;
      assert !(package.passthru.rsHarborBuildCacheWrapped or false);
        pkgs.runCommand "check-mkToolchain-cache-default-off" {} "touch $out";

    # stable channel works
    mkToolchain-stable = let
      t = self.lib.mkToolchain {
        inherit pkgs;
        channel = "stable";
      };
    in
      assert t ? rustToolchain;
      assert t ? craneLib;
        pkgs.runCommand "check-mkToolchain-stable" {} "touch $out";

    # Fleet profiles are optional, pinned by harbor-rs, and select matching
    # Cargo configuration so stable consumers do not inherit nightly -Z flags.
    mkToolchain-fleet-profiles = let
      stable = self.lib.mkToolchain {
        inherit pkgs;
        toolchainProfile = "stable";
        cache.enable = true;
      };
      nightly = self.lib.mkToolchain {
        inherit pkgs;
        toolchainProfile = "nightly";
        cache.enable = true;
      };
    in
      assert stable.toolchainProfile == "stable";
      assert stable.buildCache.contract.rustToolchain.channel == "1.98.1";
      assert stable.buildCache.contract.compiler == stable.rustToolchain.version;
      assert !(pkgs.lib.hasInfix "-Z" stable.cargoConfig.configText);
      assert nightly.toolchainProfile == "nightly";
      assert nightly.buildCache.contract.rustToolchain.channel == "nightly-2026-09-15";
      assert pkgs.lib.hasInfix "-Zthreads=0" nightly.cargoConfig.configText;
        pkgs.runCommand "check-mkToolchain-fleet-profiles" {} "touch $out";

    # A checked-in rust-toolchain.toml is authoritative and must retain its
    # declared wasm target while still adding the standard rust-analyzer
    # component.
    mkToolchain-toolchain-file = let
      t = self.lib.mkToolchain {
        inherit pkgs;
        toolchainFile = ./rust-toolchain.toml;
      };
    in
      assert t ? rustToolchain;
      assert builtins.elem "wasm32-unknown-unknown" t.crossTargets;
      assert t.rustToolchain != null;
        pkgs.runCommand "check-mkToolchain-toolchain-file" {} "touch $out";

    # Fleet profile manifests already list the cross targets; a consumer that
    # passes overlapping explicit targets must get each target and its Cargo
    # table exactly once.
    mkToolchain-deduplicates-overlapping-crossTargets = let
      t = self.lib.mkToolchain {
        inherit pkgs;
        toolchainProfile = "nightly";
        crossTargets = ["x86_64-pc-windows-gnu"];
      };
      windowsInTargets = builtins.filter (target: target == "x86_64-pc-windows-gnu") t.crossTargets;
      windowsSections =
        builtins.filter (line: line == "[target.x86_64-pc-windows-gnu]")
        (pkgs.lib.splitString "\n" t.cargoConfig.configText);
    in
      assert builtins.length windowsInTargets == 1;
      assert builtins.length windowsSections == 1;
        pkgs.runCommand "check-mkToolchain-deduplicates-overlapping-crossTargets" {} "touch $out";

    # Path-patched crates under [patch.crates-io] must not be dummified by
    # Crane's dependency-only phase. Registry dependencies may compile against
    # those patched crates and require their real API.
    craneLib-path-patch-buildPackage-disables-implicit-deps = let
      fixtureSrc = ./tests/fixtures/path-patch-fixture;
      pkg = toolchain.craneLib.buildPackage {
        src = fixtureSrc;
        pname = "path-patch-fixture";
        version = "0.1.0";
        doCheck = false;
      };
    in
      assert pkgs.lib.isDerivation pkg;
      assert pkg ? cargoArtifacts;
      assert pkg.cargoArtifacts == null;
        pkgs.runCommand "check-craneLib-path-patch-buildPackage-disables-implicit-deps" {} "touch $out";

    # Filtered source paths may not have been realised yet. Inspecting their
    # Cargo.toml must not depend on a previous build leaving the source in the
    # Nix store.
    craneLib-path-patch-filtered-source-evaluates = let
      originalSrc = ./tests/fixtures/path-patch-fixture;
      filteredSrc = pkgs.lib.cleanSourceWith {
        src = originalSrc;
        filter = _path: _type: true;
      };
      fixtureSrc =
        filteredSrc
        // {
          # Model an accessor whose filtered output is not usable yet. The
          # detector must inspect origSrc, not depend on outPath contents.
          outPath = pkgs.emptyDirectory;
          origSrc = originalSrc;
        };
      pkg = toolchain.craneLib.buildPackage {
        src = fixtureSrc;
        pname = "path-patch-filtered-source-fixture";
        version = "0.1.0";
        doCheck = false;
      };
    in
      assert pkgs.lib.isDerivation pkg;
      assert pkg ? cargoArtifacts;
      assert pkg.cargoArtifacts == null;
        pkgs.runCommand "check-craneLib-path-patch-filtered-source-evaluates" {} "touch $out";

    mkGradlePackage-shape = let
      fixture = self.lib.mkGradlePackage {
        inherit pkgs;
        pname = "gradle-fixture";
        version = "0.1.0";
        src = ./tests/fixtures/gradle-fixture;
        depsJson = ./tests/fixtures/gradle-fixture/deps.json;
        artifactPath = "build/libs/gradle-fixture-0.1.0.jar";
      };
    in
      assert pkgs.lib.isDerivation fixture;
      assert fixture ? mitmCache;
      assert fixture.rsHarbor.helper == "mkGradlePackage";
      assert fixture.rsHarbor.artifactPath == "build/libs/gradle-fixture-0.1.0.jar";
        pkgs.runCommand "check-mkGradlePackage-shape" {} "touch $out";

    mkGradlePackage-rejects-traversal = let
      result = builtins.tryEval (self.lib.mkGradlePackage {
        inherit pkgs;
        pname = "gradle-fixture";
        version = "0.1.0";
        src = ./tests/fixtures/gradle-fixture;
        depsJson = ./tests/fixtures/gradle-fixture/deps.json;
        artifactPath = "../artifact.jar";
      });
    in
      assert !result.success;
        pkgs.runCommand "check-mkGradlePackage-rejects-traversal" {} "touch $out";

    mkGradlePackage-build = self.lib.mkGradlePackage {
      inherit pkgs;
      pname = "gradle-fixture";
      version = "0.1.0";
      src = ./tests/fixtures/gradle-fixture;
      depsJson = ./tests/fixtures/gradle-fixture/deps.json;
      artifactPath = "build/libs/gradle-fixture-0.1.0.jar";
    };

    mkJetBrainsPlugin-shape = let
      fixture = self.lib.mkJetBrainsPlugin {
        inherit pkgs;
        pname = "gradle-fixture";
        version = "0.1.0";
        src = ./tests/fixtures/gradle-fixture;
        depsJson = ./tests/fixtures/gradle-fixture/deps.json;
        artifactPath = "build/libs/gradle-fixture-0.1.0.jar";
        pluginXmlId = "com.example.fixture";
      };
    in
      assert pkgs.lib.isDerivation fixture;
      assert fixture.rsHarbor.helper == "mkJetBrainsPlugin";
      assert fixture.rsHarbor.pluginXmlId == "com.example.fixture";
        pkgs.runCommand "check-mkJetBrainsPlugin-shape" {} "touch $out";

    mkJetBrainsPlugin-rejects-empty-id = let
      result = builtins.tryEval (self.lib.mkJetBrainsPlugin {
        inherit pkgs;
        pname = "gradle-fixture";
        version = "0.1.0";
        src = ./tests/fixtures/gradle-fixture;
        depsJson = ./tests/fixtures/gradle-fixture/deps.json;
        artifactPath = "build/libs/gradle-fixture-0.1.0.jar";
        pluginXmlId = "";
      });
    in
      assert !result.success;
        pkgs.runCommand "check-mkJetBrainsPlugin-rejects-empty-id" {} "touch $out";

    mkJetBrainsSigningMaterial-shape = let
      helper = self.lib.mkJetBrainsSigningMaterial {inherit pkgs;};
    in
      assert pkgs.lib.isDerivation helper;
      assert helper.name == "generate-jetbrains-signing-material";
        pkgs.runCommand "check-mkJetBrainsSigningMaterial-shape" {} "touch $out";

    # Generated sources are not safe to inspect during evaluation: their
    # output may or may not exist depending on prior store state.
    craneLib-path-patch-generated-source-rejected = let
      generatedContext = pkgs.runCommand "generated-source-context" {} ''
        mkdir -p $out
      '';
      generatedSrc =
        builtins.appendContext
        (toString ./tests/fixtures/path-patch-fixture)
        (builtins.getContext (toString generatedContext));
      result = builtins.tryEval (toolchain.craneLib.buildPackage {
        src = generatedSrc;
        pname = "generated-source-fixture";
        version = "0.1.0";
        doCheck = false;
      });
    in
      assert !result.success;
        pkgs.runCommand "check-craneLib-path-patch-generated-source-rejected" {} "touch $out";

    # Empty-context path strings are host-state inputs, not declared Nix paths.
    craneLib-path-patch-plain-string-source-rejected = let
      result = builtins.tryEval (toolchain.craneLib.buildPackage {
        src = "/tmp/undeclared-cargo-source";
        pname = "plain-string-source-fixture";
        version = "0.1.0";
        doCheck = false;
      });
    in
      assert !result.success;
        pkgs.runCommand "check-craneLib-path-patch-plain-string-source-rejected" {} "touch $out";

    # A context entry that claims both static-path and derivation-output
    # provenance is derived and must fail closed.
    craneLib-path-patch-mixed-context-source-rejected = let
      staticSrc = builtins.path {
        path = ./tests/fixtures/path-patch-fixture;
        name = "mixed-context-cargo-source";
      };
      staticString = toString staticSrc;
      contextDrv = pkgs.runCommand "mixed-context-source-dependency" {} "touch $out";
      contextKey = builtins.unsafeDiscardStringContext contextDrv.drvPath;
      mixedContext = {
        ${contextKey} = {
          path = true;
          outputs = ["out"];
        };
      };
      mixedSrc =
        builtins.appendContext
        (builtins.unsafeDiscardStringContext staticString)
        mixedContext;
      result = builtins.tryEval (toolchain.craneLib.buildPackage {
        src = mixedSrc;
        pname = "mixed-context-source-fixture";
        version = "0.1.0";
        doCheck = false;
      });
    in
      assert !result.success;
        pkgs.runCommand "check-craneLib-path-patch-mixed-context-source-rejected" {} "touch $out";

    # Callers with generated sources can provide the Cargo manifest explicitly
    # so path-patch validation stays deterministic without import-from-derivation.
    craneLib-path-patch-generated-source-explicit-manifest = let
      generatedSrc = pkgs.runCommand "generated-cargo-source" {} ''
        mkdir -p $out
      '';
      pkg = toolchain.craneLib.buildPackage {
        src = generatedSrc;
        pname = "generated-source-fixture";
        version = "0.1.0";
        doCheck = false;
        rsHarborCargoTomlContents = builtins.readFile ./tests/fixtures/path-patch-fixture/Cargo.toml;
      };
    in
      assert pkgs.lib.isDerivation pkg;
      assert pkg.cargoArtifacts == null;
        pkgs.runCommand "check-craneLib-path-patch-generated-source-explicit-manifest" {} "touch $out";

    craneLib-path-patch-buildDepsOnly-rejected = let
      fixtureSrc = ./tests/fixtures/path-patch-fixture;
      result = builtins.tryEval (toolchain.craneLib.buildDepsOnly {
        src = fixtureSrc;
        pname = "path-patch-fixture";
        version = "0.1.0";
        doCheck = false;
      });
    in
      assert result.success == false;
        pkgs.runCommand "check-craneLib-path-patch-buildDepsOnly-rejected" {} "touch $out";

    # Legacy `stdenv=` crane args must map to stdenvSelector, not warn.
    craneLib-stdenv-arg-uses-selector = let
      pkg = toolchain.craneLib.buildPackage {
        src = ./templates/default;
        pname = "stdenv-selector-fixture";
        version = "0.1.0";
        doCheck = false;
        stdenv = pkgs.clangStdenv;
      };
    in
      assert pkgs.lib.isDerivation pkg;
      assert builtins.isString pkg.drvPath;
      assert pkg.stdenv == pkgs.clangStdenv;
        pkgs.runCommand "check-craneLib-stdenv-arg-uses-selector" {} "touch $out";

    craneLib-path-patch-buildDepsOnly-escape-hatch = let
      fixtureSrc = ./tests/fixtures/path-patch-fixture;
      result = builtins.tryEval (toolchain.craneLib.buildDepsOnly {
        src = fixtureSrc;
        pname = "path-patch-fixture";
        version = "0.1.0";
        doCheck = false;
        rsHarborAllowPathPatchBuildDepsOnly = true;
      });
    in
      assert result.success == true;
      assert pkgs.lib.isDerivation result.value;
        pkgs.runCommand "check-craneLib-path-patch-buildDepsOnly-escape-hatch" {} "touch $out";

    # Rust package derivations carry compiler/linker tools explicitly so
    # build scripts and drv reproducers can find cc/gcc outside dev shells.
    mkRustNativeBuildInputs-shape = let
      inputs = self.lib.mkRustNativeBuildInputs {
        inherit pkgs;
        extra = [pkgs.pkg-config];
      };
    in
      assert builtins.elem pkgs.stdenv.cc inputs;
      assert builtins.elem pkgs.clang inputs;
      assert builtins.elem pkgs.mold inputs;
      assert builtins.elem pkgs.pkg-config inputs;
        pkgs.runCommand "check-mkRustNativeBuildInputs-shape" {} "touch $out";

    mkRustNativeBuildInputs-tool-opt-outs = let
      inputs = self.lib.mkRustNativeBuildInputs {
        inherit pkgs;
        includeClang = false;
        includeMold = false;
      };
    in
      assert builtins.elem pkgs.stdenv.cc inputs;
      assert !(builtins.elem pkgs.clang inputs);
      assert !(builtins.elem pkgs.mold inputs);
        pkgs.runCommand "check-mkRustNativeBuildInputs-tool-opt-outs" {} "touch $out";

    # The same explicit pkg-config environment is available to dev-shell
    # callers and to consumers that need to reproduce a derivation's build
    # environment outside Nix's setup-hook phase.
    mkPkgConfigEnv-shape = let
      empty = self.lib.mkPkgConfigEnv {inherit pkgs;};
      env = self.lib.mkPkgConfigEnv {
        inherit pkgs;
        deps = [pkgs.openssl];
      };
    in
      assert empty == {};
      assert env ? PKG_CONFIG_PATH;
      assert pkgs.lib.hasInfix "/lib/pkgconfig" env.PKG_CONFIG_PATH;
        pkgs.runCommand "check-mkPkgConfigEnv-shape" {} "touch $out";

    # mkCross returns expected attributes
    mkCross-shape = let
      c = self.lib.mkCross {
        inherit pkgs system;
        enableOsxcross = false;
      };
    in
      assert c ? mingwCC;
      assert c ? mingwBinutils;
      assert c ? winpthreads;
      assert c ? windowsEnv;
      assert c ? macosSdk;
      assert c ? osxcrossToolchain;
      assert c ? osxcrossRustHelpers;
      assert builtins.isAttrs c.windowsEnv;
      assert c.windowsEnv ? CC_x86_64_pc_windows_gnu;
      assert c.windowsEnv ? CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER;
        pkgs.runCommand "check-mkCross-shape" {} "touch $out";

    # osxcross disabled returns nulls
    mkCross-osxcross-disabled = let
      c = self.lib.mkCross {
        inherit pkgs system;
        enableOsxcross = false;
      };
    in
      assert c.macosSdk == null;
      assert c.osxcrossToolchain == null;
      assert c.osxcrossRustHelpers == null;
        pkgs.runCommand "check-mkCross-osxcross-disabled" {} "touch $out";

    # Pure evaluation should not enable osxcross without an SDK input.
    mkCross-osxcross-default-disabled-without-sdk = let
      c = self.lib.mkCross {inherit pkgs system;};
    in
      assert builtins.getEnv "MACOS_SDK" == "";
      assert c.macosSdk == null;
      assert c.osxcrossToolchain == null;
      assert c.osxcrossRustHelpers == null;
        pkgs.runCommand "check-mkCross-osxcross-default-disabled-without-sdk" {} "touch $out";

    # mkCross enables osxcross from a pure macOS SDK ref without MACOS_SDK.
    mkCross-osxcross-enabled-with-macos-sdk-ref =
      if system == "x86_64-linux"
      then let
        fakeMacosSdk = {
          _type = "osxcross-macos-sdk";
          sdk = pkgs.emptyDirectory;
          sdkVersion = "26.1";
          sdkRoot = "${pkgs.emptyDirectory}/MacOSX26.1.sdk";
        };
        fakeOsxcross = {
          lib.${system} = {
            mkOsxcross = args:
              args
              // {
                _type = "fake-osxcross-toolchain";
                darwinTarget = "darwin25";
                supportedArchs = ["x86_64"];
                effectiveOsxVersionMin = "10.13";
              };
            mkRustHelpers = toolchain: {
              _type = "fake-rust-helpers";
              inherit toolchain;
            };
          };
        };
        mkCross = import ./lib/cross.nix {osxcross = fakeOsxcross;};
        c = mkCross {
          inherit pkgs system;
          macosSdk = fakeMacosSdk;
        };
      in
        assert builtins.getEnv "MACOS_SDK" == "";
        assert c.macosSdk == fakeMacosSdk;
        assert c.osxcrossToolchain._type == "fake-osxcross-toolchain";
        assert c.osxcrossToolchain.macosSdk == fakeMacosSdk;
        assert c.osxcrossToolchain.sdkVersion == "26.1";
        assert c.osxcrossRustHelpers._type == "fake-rust-helpers";
          pkgs.runCommand "check-mkCross-osxcross-enabled-with-macos-sdk-ref" {} "touch $out"
      else pkgs.runCommand "check-mkCross-osxcross-enabled-with-macos-sdk-ref-skipped" {} "touch $out";

    # mkCross can initialize the pure macOS SDK ref from an archive input.
    mkCross-osxcross-enabled-with-sdk-archive =
      if system == "x86_64-linux"
      then let
        fakeSdkArchive = pkgs.emptyDirectory;
        fakeOsxcross = {
          lib.${system} = {
            mkMacosSdk = args:
              args
              // {
                _type = "osxcross-macos-sdk";
                sdk = pkgs.emptyDirectory;
                sdkRoot = "${pkgs.emptyDirectory}/MacOSX${args.sdkVersion}.sdk";
              };
            mkOsxcross = args:
              args
              // {
                _type = "fake-osxcross-toolchain";
                darwinTarget = "darwin25";
                supportedArchs = ["x86_64"];
                effectiveOsxVersionMin = "10.13";
              };
            mkRustHelpers = toolchain: {
              _type = "fake-rust-helpers";
              inherit toolchain;
            };
          };
        };
        mkCross = import ./lib/cross.nix {osxcross = fakeOsxcross;};
        c = mkCross {
          inherit pkgs system;
          sdkArchive = fakeSdkArchive;
          osxSdkVersion = "26.1";
          macosSdkOutputHash = "sha256-fake";
        };
      in
        assert c.macosSdk._type == "osxcross-macos-sdk";
        assert c.macosSdk.sdkArchive == fakeSdkArchive;
        assert c.macosSdk.sdkVersion == "26.1";
        assert c.macosSdk.outputHash == "sha256-fake";
        assert c.osxcrossToolchain.macosSdk == c.macosSdk;
        assert c.osxcrossToolchain.sdkVersion == "26.1";
        assert c.osxcrossRustHelpers.toolchain == c.osxcrossToolchain;
          pkgs.runCommand "check-mkCross-osxcross-enabled-with-sdk-archive" {} "touch $out"
      else pkgs.runCommand "check-mkCross-osxcross-enabled-with-sdk-archive-skipped" {} "touch $out";

    # mkCross resolves MACOS_SDK when it points directly at a complete .sdk directory.
    mkCross-osxcross-enabled-with-env-direct-sdk =
      if system == "x86_64-linux"
      then let
        fakeOsxcross = {
          lib.${system} = {
            mkMacosSdkRef = args:
              args
              // {
                _type = "osxcross-macos-sdk";
              };
            mkOsxcross = args:
              args
              // {
                _type = "fake-osxcross-toolchain";
                darwinTarget = "darwin25";
                supportedArchs = ["x86_64"];
                effectiveOsxVersionMin = "10.13";
              };
            mkRustHelpers = toolchain: {
              _type = "fake-rust-helpers";
              inherit toolchain;
            };
          };
        };
        mkCross = import ./lib/cross.nix {osxcross = fakeOsxcross;};
        c = mkCross {
          inherit pkgs system;
          macosSdkEnvPath = toString ./tests/fixtures/macos-sdk-complete/MacOSX26.1.sdk;
          osxSdkVersion = "26.1";
        };
      in
        assert c.macosSdk._type == "osxcross-macos-sdk";
        assert builtins.baseNameOf (toString c.macosSdk.sdkRoot) == "MacOSX26.1.sdk";
        assert c.osxcrossToolchain.macosSdk == c.macosSdk;
          pkgs.runCommand "check-mkCross-osxcross-enabled-with-env-direct-sdk" {} "touch $out"
      else pkgs.runCommand "check-mkCross-osxcross-enabled-with-env-direct-sdk-skipped" {} "touch $out";

    # mkCross resolves MACOS_SDK when it points at a parent directory containing MacOSX<version>.sdk.
    mkCross-osxcross-enabled-with-env-parent-sdk =
      if system == "x86_64-linux"
      then let
        fakeOsxcross = {
          lib.${system} = {
            mkMacosSdkRef = args:
              args
              // {
                _type = "osxcross-macos-sdk";
              };
            mkOsxcross = args:
              args
              // {
                _type = "fake-osxcross-toolchain";
                darwinTarget = "darwin25";
                supportedArchs = ["x86_64"];
                effectiveOsxVersionMin = "10.13";
              };
            mkRustHelpers = toolchain: {
              _type = "fake-rust-helpers";
              inherit toolchain;
            };
          };
        };
        mkCross = import ./lib/cross.nix {osxcross = fakeOsxcross;};
        c = mkCross {
          inherit pkgs system;
          macosSdkEnvPath = toString ./tests/fixtures/macos-sdk-complete;
          osxSdkVersion = "26.1";
        };
      in
        assert c.macosSdk._type == "osxcross-macos-sdk";
        assert builtins.baseNameOf (toString c.macosSdk.sdkRoot) == "MacOSX26.1.sdk";
        assert c.osxcrossToolchain.macosSdk == c.macosSdk;
          pkgs.runCommand "check-mkCross-osxcross-enabled-with-env-parent-sdk" {} "touch $out"
      else pkgs.runCommand "check-mkCross-osxcross-enabled-with-env-parent-sdk-skipped" {} "touch $out";

    # mkCross resolves MACOS_SDK archives through the archive workflow.
    mkCross-osxcross-enabled-with-env-sdk-archive =
      if system == "x86_64-linux"
      then let
        fakeOsxcross = {
          lib.${system} = {
            mkMacosSdk = args:
              args
              // {
                _type = "osxcross-macos-sdk";
                sdk = pkgs.emptyDirectory;
                sdkRoot = "${pkgs.emptyDirectory}/MacOSX${args.sdkVersion}.sdk";
              };
            mkOsxcross = args:
              args
              // {
                _type = "fake-osxcross-toolchain";
                darwinTarget = "darwin25";
                supportedArchs = ["x86_64"];
                effectiveOsxVersionMin = "10.13";
              };
            mkRustHelpers = toolchain: {
              _type = "fake-rust-helpers";
              inherit toolchain;
            };
          };
        };
        mkCross = import ./lib/cross.nix {osxcross = fakeOsxcross;};
        c = mkCross {
          inherit pkgs system;
          macosSdkEnvPath = toString ./tests/fixtures/MacOSX26.1.sdk.tar.xz;
          osxSdkVersion = "26.1";
        };
      in
        assert c.macosSdk._type == "osxcross-macos-sdk";
        assert builtins.baseNameOf (toString c.macosSdk.sdkArchive) == "MacOSX26.1.sdk.tar.xz";
        assert c.osxcrossToolchain.macosSdk == c.macosSdk;
          pkgs.runCommand "check-mkCross-osxcross-enabled-with-env-sdk-archive" {} "touch $out"
      else pkgs.runCommand "check-mkCross-osxcross-enabled-with-env-sdk-archive-skipped" {} "touch $out";

    # mkCross rejects incomplete SDK directories with validation diagnostics.
    mkCross-osxcross-rejects-incomplete-env-sdk =
      if system == "x86_64-linux"
      then let
        fakeOsxcross = {
          lib.${system} = {
            mkMacosSdkRef = args:
              args
              // {
                _type = "osxcross-macos-sdk";
              };
          };
        };
        mkCross = import ./lib/cross.nix {osxcross = fakeOsxcross;};
        failure =
          builtins.tryEval
          (mkCross {
            inherit pkgs system;
            macosSdkEnvPath = toString ./tests/fixtures/macos-sdk-incomplete/MacOSX26.1.sdk;
            osxSdkVersion = "26.1";
          })
        .macosSdk
        .sdkRoot;
      in
        assert failure.success == false;
          pkgs.runCommand "check-mkCross-osxcross-rejects-incomplete-env-sdk" {} "touch $out"
      else pkgs.runCommand "check-mkCross-osxcross-rejects-incomplete-env-sdk-skipped" {} "touch $out";

    # mkCross can use a VCS-compatible SDK store path produced by host init.
    mkCross-osxcross-enabled-with-sdk-store-path =
      if system == "x86_64-linux"
      then let
        fakeSdkStorePath = "/nix/store/00000000000000000000000000000000-macosx-sdk-26.1";
        fakeOsxcross = {
          lib.${system} = {
            mkMacosSdkRef = args:
              args
              // {
                _type = "osxcross-macos-sdk";
                sdkRoot = "${args.sdk}/MacOSX${args.sdkVersion}.sdk";
              };
            mkOsxcross = args:
              args
              // {
                _type = "fake-osxcross-toolchain";
                darwinTarget = "darwin25";
                supportedArchs = ["x86_64"];
                effectiveOsxVersionMin = "10.13";
              };
            mkRustHelpers = toolchain: {
              _type = "fake-rust-helpers";
              inherit toolchain;
            };
          };
        };
        mkCross = import ./lib/cross.nix {osxcross = fakeOsxcross;};
        c = mkCross {
          inherit pkgs system;
          macosSdkStorePath = fakeSdkStorePath;
          osxSdkVersion = "26.1";
        };
      in
        assert c.macosSdk._type == "osxcross-macos-sdk";
        assert toString c.macosSdk.sdk == fakeSdkStorePath;
        assert c.macosSdk.sdkVersion == "26.1";
        assert c.macosSdk.sdkRoot == "${fakeSdkStorePath}/MacOSX26.1.sdk";
        assert c.osxcrossToolchain.macosSdk == c.macosSdk;
        assert c.osxcrossRustHelpers.toolchain == c.osxcrossToolchain;
          pkgs.runCommand "check-mkCross-osxcross-enabled-with-sdk-store-path" {} "touch $out"
      else pkgs.runCommand "check-mkCross-osxcross-enabled-with-sdk-store-path-skipped" {} "touch $out";

    # The NixOS module must pass both the SDK store path and recursive hash to
    # mkCross so sandboxed builds receive the SDK as a real build input.
    nixos-module-macos-sdk-mk-cross-args = let
      evaluated = pkgs.lib.evalModules {
        modules = [
          self.nixosModules.macosSdk
          {
            options.systemd.tmpfiles.rules = pkgs.lib.mkOption {
              type = pkgs.lib.types.listOf pkgs.lib.types.str;
              default = [];
            };
          }
          {
            programs.harborRs.macosSdk = {
              enable = true;
              sdkVersion = "26.1";
              storePath = "/nix/store/00000000000000000000000000000000-macosx-sdk-26.1";
              outputHash = "sha256-fake";
            };
          }
        ];
      };
      args = evaluated.config.programs.harborRs.macosSdk.mkCrossArgs;
    in
      assert args.osxSdkVersion == "26.1";
      assert args.macosSdkStorePath == "/nix/store/00000000000000000000000000000000-macosx-sdk-26.1";
      assert args.macosSdkOutputHash == "sha256-fake";
        pkgs.runCommand "check-nixos-module-macos-sdk-mk-cross-args" {} "touch $out";

    # mkCross rejects ambiguous SDK inputs.
    mkCross-osxcross-sdk-input-conflict = let
      fakeMacosSdk = {
        _type = "osxcross-macos-sdk";
        sdk = pkgs.emptyDirectory;
        sdkVersion = "26.1";
        sdkRoot = "${pkgs.emptyDirectory}/MacOSX26.1.sdk";
      };
      conflict = builtins.tryEval (self.lib.mkCross {
        inherit pkgs system;
        sdkArchive = pkgs.emptyDirectory;
        macosSdk = fakeMacosSdk;
      });
      storeConflict = builtins.tryEval (self.lib.mkCross {
        inherit pkgs system;
        macosSdkStorePath = "/nix/store/00000000000000000000000000000000-macosx-sdk-26.1";
        sdkArchive = pkgs.emptyDirectory;
      });
    in
      assert conflict.success == false;
      assert storeConflict.success == false;
        pkgs.runCommand "check-mkCross-osxcross-sdk-input-conflict" {} "touch $out";

    validate-macos-sdk-fixtures = pkgs.runCommand "check-validate-macos-sdk-fixtures" {} ''
      "${self.packages.${system}.validate-macos-sdk}/bin/validate-macos-sdk" \
        "${./tests/fixtures/macos-sdk-complete/MacOSX26.1.sdk}" 26.1 > complete
      if "${self.packages.${system}.validate-macos-sdk}/bin/validate-macos-sdk" \
        "${./tests/fixtures/macos-sdk-incomplete/MacOSX26.1.sdk}" 26.1 > incomplete.out 2> incomplete.err
      then
        echo "incomplete SDK unexpectedly passed validation" >&2
        exit 1
      fi
      grep 'TargetConditionals.h' incomplete.err
      grep 'SystemConfiguration.framework' incomplete.err
      grep 'CoreFoundation.framework' incomplete.err
      touch "$out"
    '';

    template-default = assert defaultTemplateSimit.flake
    == {
      scope = "full";
      mode = "custom";
      backend = "rust-crane";
      toolchain_binding = "toolchain";
      crane_lib_binding = "craneLib";
      package_binding = "package";
    };
    assert pkgs.lib.hasInfix "treefmt-nix.follows" defaultTemplateFlake;
    assert pkgs.lib.hasInfix "git-hooks.follows" defaultTemplateFlake;
    assert pkgs.lib.hasInfix "treefmtEval.config.build.check self" defaultTemplateFlake;
    assert pkgs.lib.hasInfix "pre-commit-check.shellHook" defaultTemplateFlake;
    # Outputs must accept the full real input set (crane.follows still
    # passes `crane`); a closed arg set rejects its own declared inputs.
    assert pkgs.lib.hasInfix "..." defaultTemplateFlake;
    assert pkgs.lib.hasInfix "edition = \"2021\"" defaultTemplateTreefmt;
    assert pkgs.lib.hasInfix "mkRustHooks" defaultTemplateHooks;
    assert pkgs.lib.hasInfix "inherit pkgs harbor-rs" defaultTemplateFlake;
      self.lib.templateTests.mkCheck {
        inherit pkgs system;
        flakeNix = ./templates/default/flake.nix;
        inputs = {
          inherit nixpkgs rust-overlay crane treefmt-nix git-hooks;
          harbor-rs = self;
        };
        requiredFiles = [
          "flake.nix"
          "Cargo.toml"
          "Cargo.lock"
          "src/main.rs"
          "simit.toml"
          "nix/treefmt.nix"
          "nix/pre-commit.nix"
        ];
        requiredInputs = ["harbor-rs" "treefmt-nix" "git-hooks"];
        commands = ["cargo" "rustc" "rust-analyzer"];
        hookContains = ["cargo config at"];
        inherit (self.lib) devShellTests;
      };

    template-default-package = let
      outputs = self.lib.templateTests.eval {
        flakeNix = ./templates/default/flake.nix;
        inputs = {
          inherit nixpkgs rust-overlay crane treefmt-nix git-hooks;
          harbor-rs = self;
        };
      };
    in
      pkgs.runCommand "check-template-default-package" {} ''
        test -x ${outputs.packages.${system}.default}/bin/cross-fixture
        touch "$out"
      '';

    template-bevy = assert bevyTemplateSimit.flake
    == {
      scope = "full";
      mode = "custom";
      backend = "rust-crane";
      toolchain_binding = "toolchain";
      crane_lib_binding = "craneLib";
      package_binding = "default";
    };
    assert pkgs.lib.hasInfix "treefmt-nix.follows" bevyTemplateFlake;
    assert pkgs.lib.hasInfix "git-hooks.follows" bevyTemplateFlake;
    assert pkgs.lib.hasInfix "treefmtEval.config.build.check self" bevyTemplateFlake;
    assert pkgs.lib.hasInfix "pre-commit-check.shellHook" bevyTemplateFlake;
    assert pkgs.lib.hasInfix "..." bevyTemplateFlake;
    assert pkgs.lib.hasInfix "edition = \"2024\"" bevyTemplateTreefmt;
    assert pkgs.lib.hasInfix "mkRustHooks" bevyTemplateHooks;
    assert pkgs.lib.hasInfix "inherit pkgs harbor-rs" bevyTemplateFlake;
      self.lib.templateTests.mkCheck {
        inherit pkgs system;
        flakeNix = ./templates/bevy/flake.nix;
        inputs = {
          inherit nixpkgs rust-overlay crane treefmt-nix git-hooks;
          harbor-rs = self;
        };
        requiredFiles = [
          "flake.nix"
          "Cargo.toml"
          "src/main.rs"
          "nix/package.nix"
          "nix/dev-shells.nix"
          "nix/bevy-deps.nix"
          "simit.toml"
          "nix/treefmt.nix"
          "nix/pre-commit.nix"
        ];
        requiredInputs = ["harbor-rs" "treefmt-nix" "git-hooks"];
        commands = ["cargo" "rustc" "rust-analyzer"];
        hookContains = ["cargo config at"];
        inherit (self.lib) devShellTests;
      };

    # Template pre-commit files are thin forwarders to lib.hooks.mkRustHooks.
    # This evaluates both forwarders and pins the composed hook shapes: the
    # shared treefmt fragment, the two cargo hooks (clippy/audit), and the manual-only
    # nix flake check. treefmt is the sole formatter: no cargo-fmt hook.
    template-hooks-shape = let
      mkHooks = preCommitNix:
        import preCommitNix {
          inherit pkgs;
          inherit (toolchain) rustToolchain;
          treefmtWrapper = pkgs.treefmt;
          harbor-rs = self;
        };
      hooks = mkHooks ./templates/default/nix/pre-commit.nix;
      bevyHooks = mkHooks ./templates/bevy/nix/pre-commit.nix;
    in
      assert builtins.attrNames hooks == ["cargo-audit" "cargo-clippy" "nix-flake-check" "treefmt"];
      assert builtins.attrNames bevyHooks == ["cargo-audit" "cargo-clippy" "nix-flake-check" "treefmt"];
      assert !builtins.hasAttr "cargo-fmt" hooks;
      assert !builtins.hasAttr "cargo-fmt" bevyHooks;
      assert pkgs.lib.hasInfix "cargo clippy --all-targets" hooks.cargo-clippy.entry;
      assert pkgs.lib.hasInfix "cargo audit" hooks.cargo-audit.entry;
      assert pkgs.lib.hasInfix "treefmt --fail-on-change" hooks.treefmt.entry;
      assert pkgs.lib.hasInfix "cargo clippy --all-targets" bevyHooks.cargo-clippy.entry;
      assert hooks.nix-flake-check.stages == ["manual"];
        pkgs.runCommand "check-template-hooks-shape" {} "touch $out";

    # Evaluate the real pinned hook/formatter composition, rather than
    # injecting current `self` and hiding stale template locks.
    template-own-lock = import ./nix/checks/template-own-lock.nix {
      inherit pkgs;
      source = self;
      templates = {
        default = "templates/default";
        bevy = "templates/bevy";
      };
    };

    # mkDevShells returns expected shell variants
    mkDevShells-shape = let
      s = self.lib.mkDevShells {
        inherit pkgs cross;
        inherit (toolchain) craneLib;
      };
    in
      assert s ? default;
      assert s ? windows;
      assert s ? macos;
      assert s ? cross;
      assert pkgs.lib.hasInfix "cargo config at" s.default.shellHook;
        pkgs.runCommand "check-mkDevShells-shape" {} "touch $out";

    # mkDevShells accepts pkg-config dependency inputs
    mkDevShells-pkg-config-deps = let
      s = self.lib.mkDevShells {
        inherit pkgs cross;
        inherit (toolchain) craneLib;
        pkgConfigDeps = with pkgs; [
          wayland
          libxkbcommon
        ];
      };
    in
      assert s ? default;
        pkgs.runCommand "check-mkDevShells-pkg-config-deps" {} "touch $out";

    # mkDocsShell evaluates as a dedicated devShell derivation
    mkDocsShell-shape = let
      s = self.lib.mkDocsShell {
        inherit pkgs cross;
        inherit (toolchain) craneLib;
        packages = [pkgs.mdbook];
      };
    in
      assert builtins.isAttrs s;
      assert (s.type or null) == "derivation";
        pkgs.runCommand "check-mkDocsShell-shape" {} "touch $out";

    # mkProjectCliShellTools returns a package list and PATH validation hook
    # for exposing a project-built CLI in downstream dev shells.
    mkProjectCliShellTools-shape = let
      fakeCli = pkgs.writeShellApplication {
        name = "demo-cli";
        text = ''
          echo "demo-cli 1.2.3"
        '';
      };
      tools = self.lib.mkProjectCliShellTools {
        inherit pkgs;
        package = fakeCli;
        commandName = "demo-cli";
        hint = "demo-cli shell ready";
        versionCheck.expected = "1.2.3";
      };
    in
      assert tools.packages == [fakeCli];
      assert builtins.isString tools.shellHook;
      assert pkgs.lib.hasInfix "command -v" tools.shellHook;
      assert pkgs.lib.hasInfix "demo-cli" tools.shellHook;
        pkgs.runCommand "check-mkProjectCliShellTools-shape" {} "touch $out";

    # mkProjectCliShellTools puts the requested CLI on PATH and its hook
    # rejects missing/stale commands before the developer starts working.
    mkProjectCliShellTools-path-smoke = let
      fakeCli = pkgs.writeShellApplication {
        name = "demo-cli";
        text = ''
          echo "demo-cli 1.2.3"
        '';
      };
      tools = self.lib.mkProjectCliShellTools {
        inherit pkgs;
        package = fakeCli;
        commandName = "demo-cli";
        versionCheck.expected = "1.2.3";
      };
    in
      pkgs.runCommand "check-mkProjectCliShellTools-path-smoke" {
        buildInputs = tools.packages;
      } ''
        command -v demo-cli >/dev/null
        ${tools.shellHook}
        touch "$out"
      '';

    # mkDevShells accepts the full parameter set simit's cross_template()
    # passes: packages list with all audit/release tools, extraShellHook,
    # and checks.  This is the harbor-rs side of the simit↔harbor-rs
    # bidirectional contract — if this check fails, simit's generated
    # cross-compilation dev-shells are broken.
    mkDevShells-accepts-simit-parameters = let
      s = self.lib.mkDevShells {
        inherit pkgs cross;
        inherit (toolchain) craneLib;
        packages = with pkgs; [
          cargo-about
          cargo-audit
          cargo-cyclonedx
          cargo-deny
          cargo-llvm-cov
          cargo-sbom
          cargo-nextest
          cosign
          file
          gnutar
          gzip
          jq
          minisign
          nodejs
          pre-commit
          rpm
          unzip
          zip
          reprepro
          rust-analyzer
          taplo
        ];
        extraShellHook = ''
          echo "simit-generated dev-shell ready" >&2
        '';
      };
    in
      assert s ? default;
      assert s ? windows;
      assert s ? macos;
      assert s ? cross;
      # Force evaluation of each derivation to confirm the full package
      # closure resolves.  Accessing .name is sufficient — the derivation
      # is instantiated without building it.
      assert builtins.isString s.default.name;
      assert builtins.isString s.windows.name;
      assert builtins.isString s.macos.name;
      assert builtins.isString s.cross.name;
        pkgs.runCommand "check-mkDevShells-accepts-simit-parameters" {} "touch $out";

    # cargo-audit and cargo-deny resolve in nixpkgs and are available on
    # PATH when added as build inputs.  This mirrors what happens inside a
    # dev-shell built by mkDevShells — packages in `packages` end up in
    # the shell's PATH via nativeBuildInputs.
    #
    # This is the companion to the simit↔harbor-rs bidirectional contract:
    # simit's generated cross-compilation flake passes these tools to
    # mkDevShells and expects them to resolve at evaluation time.
    mkDevShells-cargo-home = let
      shell =
        (self.lib.mkDevShells {
          inherit pkgs cross;
          inherit (toolchain) craneLib;
        }).default;
      hook = pkgs.writeText "cargo-home-shell-hook" shell.shellHook;
    in
      pkgs.runCommand "check-mkDevShells-cargo-home" {
        inherit (shell) nativeBuildInputs;
      } ''
        export HOME="$TMPDIR/home"
        export XDG_CACHE_HOME="$TMPDIR/cache"
        unset CARGO_HOME
        source ${hook}
        test "$CARGO_HOME" = "$RS_HARBOR_CARGO_HOME"
        test -s "$CARGO_HOME/config.toml"
        cmp ${toolchain.cargoConfig.configPath} "$CARGO_HOME/config.toml"
        source ${hook}
        cmp ${toolchain.cargoConfig.configPath} "$CARGO_HOME/config.toml"
        chmod a-w "$CARGO_HOME"
        if source ${hook} 2>diagnostic; then
          echo 'unwritable Cargo home was ignored' >&2
          exit 1
        fi
        chmod u+w "$CARGO_HOME"
        cmp ${toolchain.cargoConfig.configPath} "$CARGO_HOME/config.toml"

        unset CARGO_HOME
        export XDG_CACHE_HOME="$TMPDIR/concurrent-cache"
        pids=()
        for i in $(seq 1 32); do
          (source ${hook} >/dev/null && cmp ${toolchain.cargoConfig.configPath} "$CARGO_HOME/config.toml") &
          pids+=("$!")
        done
        for pid in "''${pids[@]}"; do wait "$pid"; done

        export CARGO_HOME="$TMPDIR/previous-harbor-home"
        source ${hook} 2>diagnostic
        test "$CARGO_HOME" = "$TMPDIR/previous-harbor-home"
        test ! -e "$CARGO_HOME"
        grep -q 'generated Cargo config is not activated' diagnostic

        export CARGO_HOME="$TMPDIR/user-cargo"
        mkdir -p "$CARGO_HOME"
        printf '# user-owned config\n' > "$CARGO_HOME/config.toml"
        source ${hook} 2>diagnostic
        test "$CARGO_HOME" = "$TMPDIR/user-cargo"
        test "$(cat "$CARGO_HOME/config.toml")" = '# user-owned config'
        grep -q 'generated Cargo config is not activated' diagnostic

        unset CARGO_HOME
        export XDG_CACHE_HOME="$TMPDIR/not-a-directory"
        touch "$XDG_CACHE_HOME"
        if source ${hook} 2>diagnostic; then
          echo 'Cargo config installation failure was ignored' >&2
          exit 1
        fi
        touch "$out"
      '';

    mkDevShells-audit-tools-in-path =
      pkgs.runCommand "check-mkDevShells-audit-tools-in-path" {
        buildInputs = with pkgs; [cargo-audit cargo-deny cargo-sweep];
      } ''
        command -v cargo-audit >/dev/null || {
          echo "cargo-audit: not on PATH — simit requires it in every dev-shell"
          exit 1
        }
        command -v cargo-deny >/dev/null || {
          echo "cargo-deny: not on PATH — simit requires it for deny-enabled projects"
          exit 1
        }
        command -v cargo-sweep >/dev/null || {
          echo "cargo-sweep: not on PATH — harbor-rs base package is missing"
          exit 1
        }
        touch "$out"
      '';

    # mkCrossPackages returns only the requested targets keyed by output attr
    # name, and each derivation evaluates (we force drvPaths, not full builds).
    # native + windows are buildable on x86_64-linux without a macOS SDK.
    mkCrossPackages-shape = let
      fixtureSrc = ./templates/default;
      commonArgs = {
        src = fixtureSrc;
        version = "0.1.0";
        doCheck = false;
      };
      out = self.lib.mkCrossPackages {
        inherit pkgs cross commonArgs;
        inherit (toolchain) craneLib;
        pname = "fixture";
        targets = ["native" "windows"];
      };
    in
      assert out ? "fixture";
      assert out ? "fixture-windows";
      # Only the requested targets are present.
      assert !(out ? "fixture-aarch64-linux");
      assert !(out ? "fixture-darwin-x86_64");
      assert !(out ? "fixture-darwin-aarch64");
      assert pkgs.lib.isDerivation out."fixture";
      assert pkgs.lib.isDerivation out."fixture-windows";
      # Forcing drvPath proves the derivations evaluate without building Rust.
      assert builtins.isString out."fixture".drvPath;
      assert builtins.isString out."fixture-windows".drvPath;
        pkgs.runCommand "check-mkCrossPackages-shape" {} "touch $out";

    # mkCrossPackages rejects unsupported target names and commonArgs without src.
    mkCrossPackages-validation = let
      fixtureSrc = ./templates/default;
      badTarget = builtins.tryEval (self.lib.mkCrossPackages {
        inherit pkgs cross;
        inherit (toolchain) craneLib;
        pname = "fixture";
        commonArgs = {src = fixtureSrc;};
        targets = ["native" "bogus"];
      });
      missingSrc = builtins.tryEval (self.lib.mkCrossPackages {
        inherit pkgs cross;
        inherit (toolchain) craneLib;
        pname = "fixture";
        commonArgs = {version = "0.1.0";};
        targets = ["native"];
      });
    in
      assert badTarget.success == false;
      assert missingSrc.success == false;
        pkgs.runCommand "check-mkCrossPackages-validation" {} "touch $out";

    # Non-native outputs can preserve the consumer's pinned Rust channel and
    # date instead of silently switching to harbor-rs's default toolchain.
    mkCrossPackages-toolchain-args = let
      fixtureSrc = ./templates/default;
      out = self.lib.mkCrossPackages {
        inherit pkgs cross;
        inherit (toolchain) craneLib;
        pname = "fixture";
        commonArgs = {
          src = fixtureSrc;
          version = "0.1.0";
          doCheck = false;
        };
        targets = ["aarch64-linux"];
        toolchainArgs = {
          channel = "stable";
          extensions = ["rust-src"];
        };
      };
    in
      assert out ? "fixture-aarch64-linux";
      assert builtins.isString out."fixture-aarch64-linux".drvPath;
        pkgs.runCommand "check-mkCrossPackages-toolchain-args" {} "touch $out";

    # A selected native profile is inherited by non-native outputs unless the
    # consumer explicitly supplies toolchainArgs.
    mkCrossPackages-inherits-toolchain-args = let
      fixtureSrc = ./templates/default;
      mkCrossPackages = import ./lib/cross-packages.nix {
        mkToolchain = args: {
          craneLib = {
            buildDepsOnly = _: {inherit args;};
            buildPackage = packageArgs: packageArgs;
          };
        };
      };
      out = mkCrossPackages {
        inherit pkgs;
        cross = {};
        craneLib = {
          rsHarborToolchainArgs = {
            channel = "stable";
          };
        };
        pname = "fixture";
        commonArgs.src = fixtureSrc;
        targets = ["x86_64-linux-musl"];
        buildCache = null;
      };
    in
      assert out."fixture-x86_64-linux-musl".cargoArtifacts.args.channel == "stable";
        pkgs.runCommand "check-mkCrossPackages-inherits-toolchain-args" {} "touch $out";

    mkCrossPackages-cache-opt-out-reaches-target-toolchains = let
      mkCrossPackages = import ./lib/cross-packages.nix {
        mkToolchain = args: {
          craneLib = {
            buildDepsOnly = _: {inherit args;};
            buildPackage = packageArgs: packageArgs;
          };
        };
      };
      out = mkCrossPackages {
        inherit pkgs;
        cross = {};
        craneLib = {};
        pname = "fixture";
        commonArgs.src = ./templates/default;
        targets = ["x86_64-linux-musl"];
        buildCache = null;
      };
    in
      assert out."fixture-x86_64-linux-musl".cargoArtifacts.args.cache.enable == false;
        pkgs.runCommand "check-mkCrossPackages-cache-opt-out-reaches-target-toolchains" {} "touch $out";

    # mkCrossPackageOutputs preserves the flat package set and adds the
    # build/host namespace consumed by Crossbow package registries.
    mkCrossPackageOutputs-contract = let
      fixtureSrc = ./templates/default;
      out = self.lib.mkCrossPackageOutputs {
        buildSystem = "x86_64-linux";
        hostSystem = "aarch64-linux";
        inherit pkgs cross;
        inherit (toolchain) craneLib;
        pname = "fixture";
        commonArgs = {
          src = fixtureSrc;
          version = "0.1.0";
          doCheck = false;
        };
        targets = ["aarch64-linux"];
      };
    in
      assert out.crossPackages."x86_64-linux"."aarch64-linux" ? "fixture-aarch64-linux";
      assert out.packages == out.crossPackages."x86_64-linux"."aarch64-linux";
        pkgs.runCommand "check-mkCrossPackageOutputs-contract" {} "touch $out";

    # mkCrossPackages aarch64-linux + darwin targets. The aarch64 craneLib and the
    # darwin osxcross path are only meaningful on x86_64-linux, mirroring the
    # gating of the osxcross checks above. Darwin falls back to a runCommand that
    # exits 1 when no realized macOS SDK is available, so it still evaluates.
    mkCrossPackages-cross-targets =
      if system == "x86_64-linux"
      then let
        fixtureSrc = ./templates/default;
        commonArgs = {
          src = fixtureSrc;
          version = "0.1.0";
          doCheck = false;
        };
        out = self.lib.mkCrossPackages {
          inherit pkgs cross commonArgs;
          inherit (toolchain) craneLib;
          pname = "fixture";
          targets = [
            "aarch64-linux"
            "x86_64-linux-musl"
            "aarch64-linux-musl"
            "darwin-x86_64"
            "darwin-aarch64"
          ];
        };
      in
        assert out ? "fixture-aarch64-linux";
        assert out ? "fixture-x86_64-linux-musl";
        assert out ? "fixture-aarch64-linux-musl";
        assert out ? "fixture-darwin-x86_64";
        assert out ? "fixture-darwin-aarch64";
        assert !(out ? "fixture");
        assert !(out ? "fixture-windows");
        assert pkgs.lib.isDerivation out."fixture-aarch64-linux";
        assert pkgs.lib.isDerivation out."fixture-x86_64-linux-musl";
        assert pkgs.lib.isDerivation out."fixture-aarch64-linux-musl";
        assert builtins.isString out."fixture-aarch64-linux".drvPath;
        # Without a realized SDK in pure eval, darwin outputs are the
        # mkDarwinUnavailable runCommand placeholders, which still evaluate.
        assert pkgs.lib.isDerivation out."fixture-darwin-x86_64";
        assert pkgs.lib.isDerivation out."fixture-darwin-aarch64";
        assert builtins.isString out."fixture-darwin-x86_64".drvPath;
        assert builtins.isString out."fixture-darwin-aarch64".drvPath;
          pkgs.runCommand "check-mkCrossPackages-cross-targets" {} "touch $out"
      else pkgs.runCommand "check-mkCrossPackages-cross-targets-skipped" {} "touch $out";

    # Compatibility: Android helpers now live in harbor-android and are re-exported.
    harbor-android-reexports = assert self.lib ? mkAndroidSdk;
    assert self.lib ? mkAndroidDevShell;
    assert self.lib ? findLocalMavenCache;
    assert self.lib ? mkAndroidApk;
    assert self.lib ? mkAndroidApkDevBuilder;
    assert self.lib ? mkAndroidFlavorTable;
      pkgs.runCommand "check-harbor-android-reexports" {} "touch $out";

    # mkSteamRuntimeTools returns expected metadata and packages.
    # rsHarborCli is now required, so we pass the flake-built CLI here.
    mkSteamRuntimeTools-shape = let
      s = self.lib.mkSteamRuntimeTools {
        inherit pkgs;
        rsHarborCli = self.packages.${system}.harbor-rs;
      };
    in
      assert s ? runtimes;
      assert s ? selectedRuntime;
      assert s ? image;
      assert s ? containerCommandText;
      assert s ? steamRuntimeExec;
      assert s ? auditElfRuntimeDeps;
      assert s ? auditWindowsRuntimeDeps;
      assert s ? auditDarwinRuntimeDeps;
      assert s.packages ? steamRuntimeExec;
      assert s.packages ? auditElfRuntimeDeps;
      assert s.selectedRuntime.name == "Steam Linux Runtime 3.0 (sniper)";
      assert s.image == "registry.gitlab.steamos.cloud/steamrt/sniper/sdk";
      assert s ? defaultAllowRegexes;
      assert s.defaultAllowRegexes ? linuxNeeded;
      assert s.defaultAllowRegexes ? windowsDll;
      assert s.defaultAllowRegexes ? macosDylib;
      assert builtins.match s.defaultAllowRegexes.linuxNeeded "libsteam_api.so" != null;
      assert builtins.match s.defaultAllowRegexes.linuxNeeded "libc.so.6" != null;
      assert builtins.match s.defaultAllowRegexes.linuxNeeded "libsketchy.so" == null;
      assert builtins.match s.defaultAllowRegexes.windowsDll "steam_api64.dll" != null;
      assert builtins.match s.defaultAllowRegexes.windowsDll "KERNEL32.dll" != null;
      assert builtins.match s.defaultAllowRegexes.windowsDll "OPENGL32.dll" != null;
      assert builtins.match "${s.defaultAllowRegexes.macosDylib}.*" "@rpath/libfoo.dylib" != null;
      assert builtins.match "${s.defaultAllowRegexes.macosDylib}.*" "/usr/local/lib/libfoo.dylib" == null;
      assert s ? steamworksRsCargoLibraryHook;
      assert pkgs.lib.hasInfix "redistributable_bin/linux64" s.steamworksRsCargoLibraryHook;
      assert pkgs.lib.hasInfix "LD_LIBRARY_PATH" s.steamworksRsCargoLibraryHook;
      assert s ? steamRuntimeCargoBootstrap;
      assert s.packages ? steamRuntimeCargoBootstrap;
        pkgs.runCommand "check-mkSteamRuntimeTools-shape" {} ''
          "${s.steamRuntimeCargoBootstrap}/bin/steam-runtime-cargo-bootstrap" --help > help
          grep -- '--features LIST' help
          grep 'STEAM_RUNTIME_BUILD_ROOT' help
          grep 'STEAM_RUNTIME_TARGET' help
          touch "$out"
        '';

    # bootstrap-cmds-mig is exposed as a flake package
    bootstrap-cmds-mig-shape = pkgs.runCommand "check-bootstrap-cmds-mig-shape" {} ''
      mig="${self.packages.${system}.bootstrap-cmds-mig}/bin/mig"
      test -x "$mig"
      touch "$out"
    '';

    # mkOsxcrossHooks emits both shell-hook fragments
    mkOsxcrossHooks-shape = let
      h = self.lib.mkOsxcrossHooks {inherit (pkgs) llvmPackages;};
    in
      assert h ? appleClangShimsHook;
      assert h ? macosShellGuard;
      assert pkgs.lib.hasInfix "OSXCROSS_TARGET_DIR" h.appleClangShimsHook;
      assert pkgs.lib.hasInfix "RS_HARBOR_HOST_CLANG" h.appleClangShimsHook;
      assert pkgs.lib.hasInfix "OSXCROSS_SDKROOT" h.macosShellGuard;
      assert pkgs.lib.hasInfix "CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER" h.macosShellGuard;
        pkgs.runCommand "check-mkOsxcrossHooks-shape" {} "touch $out";

    # mkWindowsMsvcDevShell evaluates and produces a derivation with the
    # MSVC env vars wired up
    mkWindowsMsvcDevShell-shape = let
      s = self.lib.mkWindowsMsvcDevShell {
        inherit pkgs;
        inherit (pkgs) lib;
        inherit (pkgs) llvmPackages;
        inherit toolchain;
      };
    in
      assert pkgs.lib.isDerivation s;
      assert s ? CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER;
      assert s ? CC_x86_64_pc_windows_msvc;
      assert s ? INCLUDE;
      assert s ? LIB;
      assert pkgs.lib.hasInfix "lld-link" s.CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER;
        pkgs.runCommand "check-mkWindowsMsvcDevShell-shape" {} "touch $out";

    # mkMacosUniversalStager exposes a stager package and the cargo profile snippet.
    # The stager is a thin shim around the harbor-rs Rust CLI's
    # `harbor-rs stage macos` subcommand.
    mkMacosUniversalStager-shape = let
      s = self.lib.mkMacosUniversalStager {
        inherit pkgs;
        rsHarborCli = self.packages.${system}.harbor-rs;
      };
    in
      assert s ? stager;
      assert s ? cargoMacosPackedDebuginfoSnippet;
      assert s.packages ? stageMacosUniversal;
      assert pkgs.lib.hasInfix "split-debuginfo = \"packed\"" s.cargoMacosPackedDebuginfoSnippet;
        pkgs.runCommand "check-mkMacosUniversalStager-shape" {} ''
          "${s.stager}/bin/stage-macos-universal" --help > help
          grep -- '--binary <BINARY>' help
          grep -- '--dylib <DYLIB>' help
          grep -- '--archs <ARCHS>' help
          touch "$out"
        '';

    # The harbor-rs CLI exposes top-level subcommands.
    harbor-rs-cli-shape = pkgs.runCommand "check-harbor-rs-cli-shape" {} ''
      "${self.packages.${system}.harbor-rs}/bin/harbor-rs" --help > help
      grep 'harbor-rs build helpers' help
      grep 'stage' help
      grep 'audit' help
      "${self.packages.${system}.harbor-rs}/bin/harbor-rs" stage --help > stage-help
      grep 'macos' stage-help
      "${self.packages.${system}.harbor-rs}/bin/harbor-rs" audit --help > audit-help
      grep 'elf' audit-help
      grep 'pe' audit-help
      grep 'macho' audit-help
      "${self.packages.${system}.harbor-rs}/bin/harbor-rs" audit elf --help > elf-help
      grep -- '--allow-needed-regex' elf-help
      grep -- '--require-origin-rpath' elf-help
      grep -- '--skip-ldd' elf-help
      "${self.packages.${system}.harbor-rs}/bin/harbor-rs" audit pe --help > pe-help
      grep -- '--allow-dll-regex' pe-help
      "${self.packages.${system}.harbor-rs}/bin/harbor-rs" audit macho --help > macho-help
      grep -- '--allow-dylib-regex' macho-help
      "${self.packages.${system}.harbor-rs}/bin/harbor-rs" steam-runtime --help > sr-help
      grep 'exec' sr-help
      "${self.packages.${system}.harbor-rs}/bin/harbor-rs" steam-runtime exec --help > sr-exec-help
      grep -- '--image' sr-exec-help
      grep -- '--container-runtime' sr-exec-help
      grep -- '--mount-nix-store' sr-exec-help
      touch "$out"
    '';

    # When mkSteamRuntimeTools is given an rsHarborCli, the audit helpers
    # become thin shims around `harbor-rs audit ...`.
    mkSteamRuntimeTools-rust-audit-shims = let
      s = self.lib.mkSteamRuntimeTools {
        inherit pkgs;
        rsHarborCli = self.packages.${system}.harbor-rs;
      };
    in
      pkgs.runCommand "check-mkSteamRuntimeTools-rust-audit-shims" {} ''
        "${s.auditElfRuntimeDeps}/bin/audit-elf-runtime-deps" --help > elf-help
        grep 'harbor-rs audit elf' elf-help
        grep -- '--allow-needed-regex' elf-help
        "${s.auditWindowsRuntimeDeps}/bin/audit-windows-runtime-deps" --help > pe-help
        grep -- '--allow-dll-regex' pe-help
        "${s.auditDarwinRuntimeDeps}/bin/audit-darwin-runtime-deps" --help > macho-help
        grep -- '--allow-dylib-regex' macho-help
        touch "$out"
      '';

    # mkSteamRuntimeTools threads steamworksRsLibSubdir through the cargo hook
    mkSteamRuntimeTools-steamworks-subdir = let
      s = self.lib.mkSteamRuntimeTools {
        inherit pkgs;
        rsHarborCli = self.packages.${system}.harbor-rs;
        steamworksRsLibSubdir = "osx";
      };
    in
      assert pkgs.lib.hasInfix "redistributable_bin/osx" s.steamworksRsCargoLibraryHook;
        pkgs.runCommand "check-mkSteamRuntimeTools-steamworks-subdir" {} "touch $out";

    # mkSteamRuntimeTools supports custom images without changing downstream policy
    mkSteamRuntimeTools-custom-image = let
      s = self.lib.mkSteamRuntimeTools {
        inherit pkgs;
        rsHarborCli = self.packages.${system}.harbor-rs;
        runtime = "custom";
        customImage = "registry.example.com/custom/steam-sdk:latest";
        containerRuntime = "docker";
      };
    in
      assert s.selectedRuntime.name == "custom";
      assert s.image == "registry.example.com/custom/steam-sdk:latest";
      assert pkgs.lib.hasInfix "docker run" s.containerCommandText;
        pkgs.runCommand "check-mkSteamRuntimeTools-custom-image" {} "touch $out";

    # mkCargoConfig returns expected attributes
    mkCargoConfig-shape = let
      c = self.lib.mkCargoConfig {inherit pkgs;};
    in
      assert c ? configText;
      assert c ? configPath;
      assert builtins.isString c.configText;
        pkgs.runCommand "check-mkCargoConfig-shape" {} "touch $out";

    # mkCargoConfig stable disables nightly features by default
    mkCargoConfig-stable = let
      c = self.lib.mkCargoConfig {
        inherit pkgs;
        channel = "stable";
      };
    in
      assert !(pkgs.lib.hasInfix "cranelift" c.configText);
      assert !(pkgs.lib.hasInfix "share-generics" c.configText);
      assert !(pkgs.lib.hasInfix "threads" c.configText);
        pkgs.runCommand "check-mkCargoConfig-stable" {} "touch $out";

    # Stable Cargo configurations cannot opt into the nightly-only backend.
    mkCargoConfig-stable-rejects-cranelift = let
      result =
        builtins.tryEval
        (self.lib.mkCargoConfig {
          inherit pkgs;
          channel = "stable";
          enableCranelift = true;
        }).configText;
    in
      assert !result.success;
        pkgs.runCommand "check-mkCargoConfig-stable-rejects-cranelift" {} "touch $out";

    # mkCargoConfig nightly includes safe-by-default optimizations
    mkCargoConfig-nightly = let
      c = self.lib.mkCargoConfig {
        inherit pkgs;
        channel = "nightly";
      };
    in
      assert !(pkgs.lib.hasInfix "codegen-backend" c.configText);
      assert pkgs.lib.hasInfix "share-generics" c.configText;
      assert pkgs.lib.hasInfix "mold" c.configText;
      assert pkgs.lib.hasInfix "threads" c.configText;
        pkgs.runCommand "check-mkCargoConfig-nightly" {} "touch $out";

    # Cranelift remains available to nightly projects that verify compatibility.
    mkCargoConfig-cranelift-opt-in = let
      c = self.lib.mkCargoConfig {
        inherit pkgs;
        channel = "nightly";
        enableCranelift = true;
      };
    in
      assert pkgs.lib.hasInfix ''codegen-backend = "cranelift"'' c.configText;
      assert pkgs.lib.hasInfix ''[profile.dev.package."*"]'' c.configText;
      assert pkgs.lib.hasInfix ''codegen-backend = "llvm"'' c.configText;
        pkgs.runCommand "check-mkCargoConfig-cranelift-opt-in" {} "touch $out";

    # Duplicate crossTargets entries must not emit duplicate Cargo tables.
    mkCargoConfig-deduplicates-targets = let
      c = self.lib.mkCargoConfig {
        inherit pkgs;
        channel = "nightly";
        crossTargets = ["x86_64-pc-windows-gnu" "x86_64-pc-windows-gnu"];
      };
      windowsSections =
        builtins.filter (line: line == "[target.x86_64-pc-windows-gnu]")
        (pkgs.lib.splitString "\n" c.configText);
    in
      assert builtins.length windowsSections == 1;
        pkgs.runCommand "check-mkCargoConfig-deduplicates-targets" {} "touch $out";

    # mkCargoConfig enables memory-saving dev profile defaults by default
    mkCargoConfig-dev-profile-opts = let
      enabled = self.lib.mkCargoConfig {inherit pkgs;};
      disabled = self.lib.mkCargoConfig {
        inherit pkgs;
        enableDevProfileOpts = false;
        devCodegenUnits = null;
      };
    in
      assert pkgs.lib.hasInfix "[profile.dev]" enabled.configText;
      assert pkgs.lib.hasInfix ''debug = "line-tables-only"'' enabled.configText;
      assert pkgs.lib.hasInfix ''split-debuginfo = "unpacked"'' enabled.configText;
      assert pkgs.lib.hasInfix ''[profile.dev.package."*"]'' enabled.configText;
      assert !(pkgs.lib.hasInfix "codegen-units = " enabled.configText);
      assert !(pkgs.lib.hasInfix ''debug = "line-tables-only"'' disabled.configText);
      assert !(pkgs.lib.hasInfix "split-debuginfo" disabled.configText);
      assert !(pkgs.lib.hasInfix "codegen-units" disabled.configText);
      assert !(pkgs.lib.hasInfix "debug = false" disabled.configText);
        pkgs.runCommand "check-mkCargoConfig-dev-profile-opts" {} "touch $out";

    # mkCargoConfig emits dev codegen-units only when explicitly requested
    mkCargoConfig-dev-codegen-units = let
      c = self.lib.mkCargoConfig {
        inherit pkgs;
        devCodegenUnits = 16;
      };
    in
      assert pkgs.lib.hasInfix "codegen-units = 16" c.configText;
        pkgs.runCommand "check-mkCargoConfig-dev-codegen-units" {} "touch $out";

    # mkCargoConfig respects enableMold = false
    mkCargoConfig-no-mold = let
      c = self.lib.mkCargoConfig {
        inherit pkgs;
        enableMold = false;
      };
    in
      assert !(pkgs.lib.hasInfix "mold" c.configText);
        pkgs.runCommand "check-mkCargoConfig-no-mold" {} "touch $out";

    # Windows GNU should not force lld through the MinGW GCC wrapper
    mkCargoConfig-no-windows-gnu-lld = let
      c = self.lib.mkCargoConfig {inherit pkgs;};
    in
      assert !(pkgs.lib.hasInfix "link-arg=-fuse-ld=lld" c.configText);
        pkgs.runCommand "check-mkCargoConfig-no-windows-gnu-lld" {} "touch $out";

    # mkTrunkPackage includes the host linker tools needed by harbor-rs's
    # generated Cargo config when Linux mold support is enabled.
    mkTrunkPackage-includes-linker-tools = let
      cargoLock = ./tests/fixtures/dioxus-fixture/Cargo.lock;
      drv = self.lib.mkTrunkPackage {
        inherit pkgs;
        src = ./tests/fixtures/dioxus-fixture;
        inherit cargoLock;
        pname = "check-trunk-linker-tools";
        inherit (toolchain) craneLib;
      };
    in
      assert builtins.elem pkgs.clang drv.nativeBuildInputs;
      assert builtins.elem pkgs.mold drv.nativeBuildInputs;
      assert drv ? artifactBuilder;
      assert drv.artifactBuilder.kind == "trunk-builder";
      assert drv.artifactBuilder.output == toString drv;
        pkgs.runCommand "check-mkTrunkPackage-includes-linker-tools" {} "touch $out";

    # Dioxus packaging keeps the generic bundle and artifact contract in
    # harbor-rs; downstream projects only provide their Cargo source policy.
    mkDioxusPackage-shape = let
      wasmToolchain = self.lib.mkWasmToolchain {inherit pkgs;};
      cargoLock = ./tests/fixtures/dioxus-fixture/Cargo.lock;
      vendor = pkgs.runCommand "dioxus-check-vendor" {} ''
        mkdir -p $out
        : > $out/config.toml
      '';
      drv = self.lib.mkDioxusPackage {
        inherit pkgs cargoLock;
        src = ./tests/fixtures/dioxus-fixture;
        pname = "check-dioxus-package";
        inherit (toolchain) craneLib;
        inherit (wasmToolchain) rustToolchain;
        cargoVendorDir = vendor;
        # This nixpkgs snapshot does not expose 0.2.126 as an attribute. The
        # explicit opt-out is intentional here; production callers should
        # provide a custom exact-version derivation instead.
        wasmBindgenCli = pkgs.wasm-bindgen-cli_0_2_120;
        allowWasmBindgenMismatch = true;
      };
      attrs = (drv.drvAttrs.env or {}) // drv.drvAttrs;
    in
      assert drv ? artifactBuilder;
      assert drv.artifactBuilder.kind == "trunk-builder";
      assert builtins.elem pkgs.clang drv.nativeBuildInputs;
      assert builtins.elem pkgs.mold drv.nativeBuildInputs;
      assert drv.artifactBuilder.metadata.helper == "mkDioxusPackage";
      assert drv.artifactBuilder.metadata.wasmSplit == false;
      assert drv.passthru.dioxus.wasmBindgenVersion == "0.2.126";
      assert !(attrs ? RUSTC_WRAPPER);
        pkgs.runCommand "check-mkDioxusPackage-shape" {} "touch $out";

    mkDioxusAssetLinker-shape = let
      linker = self.lib.mkDioxusAssetLinker {inherit pkgs;};
    in
      pkgs.runCommand "check-mkDioxusAssetLinker-shape" {
        nativeBuildInputs = [linker];
      } ''
        test -x ${linker}/bin/dioxus-link-assets
        if dioxus-link-assets 2>usage; then
          echo "dioxus-link-assets unexpectedly accepted missing arguments" >&2
          exit 1
        else
          status=$?
        fi
        test "$status" -eq 64
        grep -Fq 'usage: dioxus-link-assets EXECUTABLE DESTINATION' usage
        touch $out
      '';

    mkDioxusFullstackPackage-fixture = let
      wasmToolchain = self.lib.mkWasmToolchain {inherit pkgs;};
      # This nixpkgs snapshot predates the fixture's locked 0.2.126 CLI
      # attribute. Build the exact CLI once for the real fixture instead of
      # allowing Dioxus' runtime ABI check to be bypassed.
      wasmBindgenCli = pkgs.rustPlatform.buildRustPackage {
        pname = "wasm-bindgen-cli";
        version = "0.2.126";
        src = pkgs.fetchCrate {
          pname = "wasm-bindgen-cli";
          version = "0.2.126";
          hash = "sha256-H6Is3fiZVxZCfOMWK5dWMSrtn50VGv0sfdnsT+cTtyk=";
        };
        cargoHash = "sha256-VucqkXbCi4qtQzY/HrXiDnbSURsagPsdNVMn1Tw3UiY=";
        nativeBuildInputs = [pkgs.pkg-config];
        buildInputs = [pkgs.openssl] ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isDarwin [pkgs.curl];
        doCheck = false;
      };
      drv = self.lib.mkDioxusFullstackPackage {
        inherit pkgs;
        src = ./tests/fixtures/dioxus-fixture;
        cargoLock = ./tests/fixtures/dioxus-fixture/Cargo.lock;
        pname = "check-dioxus-fullstack";
        inherit (toolchain) craneLib;
        inherit (wasmToolchain) rustToolchain;
        inherit wasmBindgenCli;
        package = "dioxus-harbor-fixture";
        webFeatures = ["web"];
        serverFeatures = ["server"];
      };
    in
      pkgs.runCommand "check-mkDioxusFullstackPackage-fixture" {
        nativeBuildInputs = [pkgs.wabt];
      } ''
        test -x ${drv}/bin/check-dioxus-fullstack
        test -s ${drv}/share/check-dioxus-fullstack/public/index.html
        test -d ${drv}/share/check-dioxus-fullstack/public/assets
        js=$(find ${drv}/share/check-dioxus-fullstack/public/assets -type f -name '*.js' -print -quit)
        wasm=$(find ${drv}/share/check-dioxus-fullstack/public/assets -type f -name '*.wasm' -print -quit)
        test -n "$js"
        test -n "$wasm"
        grep -Fq "$(basename "$js")" ${drv}/share/check-dioxus-fullstack/public/index.html
        if wasm-objdump -h "$wasm" | grep -Fq '.debug_info'; then
          echo "release Dioxus bundle retained DWARF debug information" >&2
          exit 1
        fi
        touch $out
      '';

    mkDioxusBuildPlan-fixture = let
      plan = self.lib.mkDioxusBuildPlan {
        inherit (pkgs) lib;
        package = "dioxus-harbor-fixture";
        sharedFeatures = [];
        webFeatures = ["web"];
        serverFeatures = ["server"];
        wasmSplit = false;
        fullstack = true;
      };
    in
      assert builtins.elem "--fullstack" plan.dxArgs;
      assert builtins.elem "@client" plan.dxArgs;
      assert builtins.elem "@server" plan.dxArgs;
      assert builtins.elem "--server" plan.dxArgs;
      assert builtins.elem "--target" plan.dxArgs;
      assert builtins.elem "--debug-symbols=false" plan.dxArgs;
      assert builtins.elem "--features" plan.webCargoArgs;
      assert plan.metadata.featureSets.server == ["server"];
      assert plan.metadata.wasmTarget == "wasm32-unknown-unknown";
      assert !plan.metadata.debugSymbols;
        pkgs.runCommand "check-mkDioxusBuildPlan-fixture" {} "touch $out";

    resolveWasmBindgenCli-rejects-mismatch = let
      result = builtins.tryEval (self.lib.resolveWasmBindgenCli {
        inherit pkgs;
        inherit (pkgs) lib;
        cargoLock = ./tests/fixtures/dioxus-fixture/Cargo.lock;
        wasmBindgenCli = pkgs.wasm-bindgen-cli_0_2_120;
      });
    in
      assert !result.success;
        pkgs.runCommand "check-resolveWasmBindgenCli-rejects-mismatch" {} "touch $out";

    # mkCargoConfig respects enableParallelFrontend = false
    mkCargoConfig-no-parallel-frontend = let
      c = self.lib.mkCargoConfig {
        inherit pkgs;
        enableParallelFrontend = false;
      };
    in
      assert !(pkgs.lib.hasInfix "threads" c.configText);
        pkgs.runCommand "check-mkCargoConfig-no-parallel-frontend" {} "touch $out";

    # mkAdapter returns expected attributes
    mkAdapter-shape = let
      a = self.lib.mkAdapter {
        attic = {
          endpoint = "https://cache.example.com";
          cache = "main";
        };
      };
    in
      assert a ? _type;
      assert a._type == "harbor-adapter";
      assert a ? _version;
      assert a._version == 1;
      assert a ? attic;
      assert a.attic ? endpoint;
      assert a.attic ? cache;
      assert a.attic ? tokenEnvVar;
      assert a.attic.tokenEnvVar == "ATTIC_TOKEN";
      assert a.attic ? serverName;
      assert a.attic.serverName == "harbor";
        pkgs.runCommand "check-mkAdapter-shape" {} "touch $out";

    # mkAdapter preserves custom tokenEnvVar
    mkAdapter-custom-token = let
      a = self.lib.mkAdapter {
        attic = {
          endpoint = "https://cache.example.com";
          cache = "main";
          tokenEnvVar = "MY_TOKEN";
        };
      };
    in
      assert a.attic.tokenEnvVar == "MY_TOKEN";
        pkgs.runCommand "check-mkAdapter-custom-token" {} "touch $out";

    # isHarborAdapter accepts valid adapters
    isHarborAdapter-valid = let
      a = self.lib.mkAdapter {
        attic = {
          endpoint = "https://x.com";
          cache = "c";
        };
      };
    in
      assert self.lib.isHarborAdapter a;
        pkgs.runCommand "check-isHarborAdapter-valid" {} "touch $out";

    # isHarborAdapter rejects invalid values
    isHarborAdapter-invalid = assert !(self.lib.isHarborAdapter {});
    assert !(self.lib.isHarborAdapter {_type = "wrong";});
    assert !(self.lib.isHarborAdapter "string");
    assert !(self.lib.isHarborAdapter 42);
      pkgs.runCommand "check-isHarborAdapter-invalid" {} "touch $out";

    # mkAtticPush returns a valid app attrset
    mkAtticPush-shape = let
      adapter = self.lib.mkAdapter {
        attic = {
          endpoint = "https://x.com";
          cache = "c";
        };
      };
      app = self.lib.mkAtticPush {
        inherit pkgs adapter;
        paths = [pkgs.hello];
      };
    in
      assert app ? type;
      assert app.type == "app";
      assert app ? program;
      assert builtins.isString app.program;
        pkgs.runCommand "check-mkAtticPush-shape" {} "touch $out";

    # mkCoprSpec returns expected attributes with default binary-shipping layout
    mkCoprSpec-shape = let
      s = self.lib.mkCoprSpec {
        inherit pkgs;
        name = "test-app";
        version = "1.0.0";
        summary = "Test summary";
        license = "MIT";
      };
    in
      assert s ? specText;
      assert s ? specPath;
      assert s ? artifactBuilder;
      assert s.artifactBuilder.kind == "copr-rpm-builder";
      assert s.artifactBuilder.output == toString s.specPath;
      assert !(s ? coprMakefilePath);
      assert builtins.isString s.specText;
      assert pkgs.lib.hasInfix "Name:    test-app" s.specText;
      assert pkgs.lib.hasInfix "Version: 1.0.0" s.specText;
      assert pkgs.lib.hasInfix "License: MIT" s.specText;
      assert pkgs.lib.hasInfix "Source0: %{name}-%{version}.tar.gz" s.specText;
      assert pkgs.lib.hasInfix "%{_bindir}/%{name}" s.specText;
      assert pkgs.lib.hasInfix "%description\nTest summary" s.specText;
        pkgs.runCommand "check-mkCoprSpec-shape" {} "touch $out";

    # mkCoprSpec includes desktop + icon install rules when provided
    mkCoprSpec-desktop-icon = let
      s = self.lib.mkCoprSpec {
        inherit pkgs;
        name = "myapp";
        version = "2.3";
        summary = "Demo";
        license = "Apache-2.0";
        appId = "com.example.MyApp";
        desktopFile = "[Desktop Entry]\nType=Application";
        icon = "/some/path/myapp.png";
      };
    in
      assert pkgs.lib.hasInfix "com.example.MyApp.desktop" s.specText;
      assert pkgs.lib.hasInfix "hicolor/256x256/apps/com.example.MyApp.png" s.specText;
      assert pkgs.lib.hasInfix "install -Dm644 myapp.png" s.specText;
        pkgs.runCommand "check-mkCoprSpec-desktop-icon" {} "touch $out";

    # mkCoprSpec emits a custom-build Makefile when requested
    mkCoprSpec-copr-makefile = let
      s = self.lib.mkCoprSpec {
        inherit pkgs;
        name = "withmake";
        version = "0.1";
        summary = "x";
        license = "MIT";
        coprMakefile = true;
      };
    in
      assert s ? coprMakefilePath;
        pkgs.runCommand "check-mkCoprSpec-copr-makefile" {} ''
          grep 'SPEC := withmake.spec' ${s.coprMakefilePath}
          grep '^srpm:' ${s.coprMakefilePath}
          grep 'rpmbuild -bs' ${s.coprMakefilePath}
          touch $out
        '';

    # mkCoprSpec renders changelog entries in RPM format
    mkCoprSpec-changelog = let
      s = self.lib.mkCoprSpec {
        inherit pkgs;
        name = "cl";
        version = "1.2";
        summary = "x";
        license = "MIT";
        changelog = [
          {
            date = "Tue May 19 2026";
            author = "Can <can@example.com>";
            version = "1.2-1";
            entries = ["Bump to 1.2" "Fix the bug"];
          }
        ];
      };
    in
      assert pkgs.lib.hasInfix "%changelog" s.specText;
      assert pkgs.lib.hasInfix "* Tue May 19 2026 Can <can@example.com> - 1.2-1" s.specText;
      assert pkgs.lib.hasInfix "- Bump to 1.2" s.specText;
      assert pkgs.lib.hasInfix "- Fix the bug" s.specText;
        pkgs.runCommand "check-mkCoprSpec-changelog" {} "touch $out";

    # mkCoprSpec rejects an RPM-illegal version (contains '-')
    mkCoprSpec-rejects-bad-version = let
      result = builtins.tryEval (self.lib.mkCoprSpec {
        inherit pkgs;
        name = "x";
        version = "1.0-beta";
        summary = "x";
        license = "MIT";
      });
    in
      assert !result.success;
        pkgs.runCommand "check-mkCoprSpec-rejects-bad-version" {} "touch $out";

    # mkCoprSpec rejects a non-reverse-DNS appId
    mkCoprSpec-rejects-bad-appid = let
      result = builtins.tryEval (self.lib.mkCoprSpec {
        inherit pkgs;
        name = "x";
        version = "1";
        summary = "x";
        license = "MIT";
        appId = "notReverseDns";
      });
    in
      assert !result.success;
        pkgs.runCommand "check-mkCoprSpec-rejects-bad-appid" {} "touch $out";

    # Validation logic works correctly
    validation-helpers = let
      dateRegex = "[0-9]{4}-[0-9]{2}-[0-9]{2}";
    in
      assert builtins.match dateRegex "2025-12-01" != null;
      assert builtins.match dateRegex "yesterday" == null;
      assert builtins.match dateRegex "25-12-01" == null;
      assert builtins.elem "nightly" ["nightly" "stable"];
      assert builtins.elem "stable" ["nightly" "stable"];
      assert !(builtins.elem "beta" ["nightly" "stable"]);
        pkgs.runCommand "check-validation-helpers" {} "touch $out";

    # mkDebPackage exposes its package artifact through the builder hierarchy.
    mkDebPackage-shape = let
      deb = self.lib.mkDebPackage {
        inherit pkgs;
        packageName = "demo";
        version = "1.0.0";
        arch = "amd64";
        maintainer = "Example <example@example.com>";
        description = "Demo package";
        files = [
          {
            source = pkgs.writeText "demo" "demo";
            target = "/usr/share/demo/demo.txt";
          }
        ];
      };
    in
      assert deb ? artifactBuilder;
      assert deb.artifactBuilder.kind == "debian-builder";
      assert deb.artifactBuilder.output == toString deb;
        pkgs.runCommand "check-mkDebPackage-shape" {} "touch $out";

    # mkHomebrewFormula returns expected attributes and a binary install block.
    mkHomebrewFormula-shape = let
      f = self.lib.mkHomebrewFormula {
        inherit pkgs;
        name = "modde";
        version = "1.2.3";
        description = "Cross-platform game mod manager";
        homepage = "https://modde.tartanoglu.com";
        license = "MIT";
        platforms.darwin_arm = {
          url = "https://codeberg.org/caniko/modde/releases/download/1.2.3/modde-1.2.3-aarch64-darwin.tar.gz";
          sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        };
        dependencies = ["openssl@3"];
        binaries = ["modde" "modde-ui"];
        testBlock = ''system "#{bin}/modde", "--version"'';
      };
    in
      assert f ? formulaText;
      assert f ? formulaPath;
      assert f ? artifactBuilder;
      assert f.artifactBuilder.kind == "homebrew-builder";
      assert f.artifactBuilder.output == toString f.formulaPath;
      assert builtins.isString f.formulaText;
      assert pkgs.lib.hasInfix "class Modde < Formula" f.formulaText;
      assert pkgs.lib.hasInfix "def install" f.formulaText;
      assert pkgs.lib.hasInfix ''bin.install "modde"'' f.formulaText;
      assert pkgs.lib.hasInfix ''bin.install "modde-ui"'' f.formulaText;
      assert pkgs.lib.hasInfix ''depends_on "openssl@3"'' f.formulaText;
        pkgs.runCommand "check-mkHomebrewFormula-shape" {} ''
          grep 'class Modde < Formula' ${f.formulaPath}
          grep 'def install' ${f.formulaPath}
          touch $out
        '';

    # mkHomebrewFormula nests architecture stanzas under OS platform blocks.
    mkHomebrewFormula-multi-platform = let
      both = self.lib.mkHomebrewFormula {
        inherit pkgs;
        name = "multi-app";
        version = "1.0.0";
        description = "Multi platform app";
        homepage = "https://example.com";
        license = "Apache-2.0";
        platforms = {
          darwin_arm = {
            url = "https://example.com/multi-app-1.0.0-aarch64-darwin.tar.gz";
            sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
          };
          linux_intel = {
            url = "https://example.com/multi-app-1.0.0-x86_64-linux.tar.gz";
            sha256 = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
          };
        };
      };
      macOnly = self.lib.mkHomebrewFormula {
        inherit pkgs;
        name = "mac-app";
        version = "1.0.0";
        description = "Mac only app";
        homepage = "https://example.com";
        license = "MIT";
        platforms.darwin_intel = {
          url = "https://example.com/mac-app-1.0.0-x86_64-darwin.tar.gz";
          sha256 = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
        };
      };
    in
      assert pkgs.lib.hasInfix "on_macos do" both.formulaText;
      assert pkgs.lib.hasInfix "on_linux do" both.formulaText;
      assert pkgs.lib.hasInfix "on_arm do" both.formulaText;
      assert pkgs.lib.hasInfix "on_intel do" both.formulaText;
      assert pkgs.lib.hasInfix "on_macos do" macOnly.formulaText;
      assert !(pkgs.lib.hasInfix "on_linux do" macOnly.formulaText);
        pkgs.runCommand "check-mkHomebrewFormula-multi-platform" {} "touch $out";

    # mkHomebrewFormula emits Homebrew's checksum skip symbol for placeholders.
    mkHomebrewFormula-no-check-placeholder = let
      f = self.lib.mkHomebrewFormula {
        inherit pkgs;
        name = "placeholder-app";
        version = "1.0.0";
        description = "Placeholder checksum app";
        homepage = "https://example.com";
        license = "MIT";
        platforms.linux_intel = {
          url = "https://example.com/placeholder-app-1.0.0-x86_64-linux.tar.gz";
          sha256 = ":no_check";
        };
      };
    in
      assert pkgs.lib.hasInfix "sha256 :no_check" f.formulaText;
      assert !(pkgs.lib.hasInfix ''sha256 ":no_check"'' f.formulaText);
        pkgs.runCommand "check-mkHomebrewFormula-no-check-placeholder" {} "touch $out";

    # mkHomebrewFormula rejects a Homebrew-illegal formula name.
    mkHomebrewFormula-rejects-bad-name = let
      result = builtins.tryEval (self.lib.mkHomebrewFormula {
        inherit pkgs;
        name = "Bad_Name";
        version = "1.0.0";
        description = "Bad name app";
        homepage = "https://example.com";
        license = "MIT";
        platforms.linux_intel = {
          url = "https://example.com/bad-name.tar.gz";
          sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        };
      });
    in
      assert !result.success;
        pkgs.runCommand "check-mkHomebrewFormula-rejects-bad-name" {} "touch $out";

    # mkHomebrewFormula rejects versions with a leading v.
    mkHomebrewFormula-rejects-bad-version = let
      result = builtins.tryEval (self.lib.mkHomebrewFormula {
        inherit pkgs;
        name = "bad-version";
        version = "v1.0.0";
        description = "Bad version app";
        homepage = "https://example.com";
        license = "MIT";
        platforms.linux_intel = {
          url = "https://example.com/bad-version.tar.gz";
          sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        };
      });
    in
      assert !result.success;
        pkgs.runCommand "check-mkHomebrewFormula-rejects-bad-version" {} "touch $out";

    # mkHomebrewFormula rejects descriptions longer than Homebrew audit allows.
    mkHomebrewFormula-rejects-long-desc = let
      result = builtins.tryEval (self.lib.mkHomebrewFormula {
        inherit pkgs;
        name = "long-desc";
        version = "1.0.0";
        description = "This description is deliberately longer than eighty characters so validation rejects it.";
        homepage = "https://example.com";
        license = "MIT";
        platforms.linux_intel = {
          url = "https://example.com/long-desc.tar.gz";
          sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        };
      });
    in
      assert !result.success;
        pkgs.runCommand "check-mkHomebrewFormula-rejects-long-desc" {} "touch $out";

    # mkHomebrewFormula rejects non-HTTPS homepages.
    mkHomebrewFormula-rejects-http-homepage = let
      result = builtins.tryEval (self.lib.mkHomebrewFormula {
        inherit pkgs;
        name = "http-homepage";
        version = "1.0.0";
        description = "HTTP homepage app";
        homepage = "http://example.com";
        license = "MIT";
        platforms.linux_intel = {
          url = "https://example.com/http-homepage.tar.gz";
          sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        };
      });
    in
      assert !result.success;
        pkgs.runCommand "check-mkHomebrewFormula-rejects-http-homepage" {} "touch $out";

    # mkHomebrewFormula rejects unsupported platform keys.
    mkHomebrewFormula-rejects-bad-platform = let
      result = builtins.tryEval (self.lib.mkHomebrewFormula {
        inherit pkgs;
        name = "bad-platform";
        version = "1.0.0";
        description = "Bad platform app";
        homepage = "https://example.com";
        license = "MIT";
        platforms.freebsd_intel = {
          url = "https://example.com/bad-platform.tar.gz";
          sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        };
      });
    in
      assert !result.success;
        pkgs.runCommand "check-mkHomebrewFormula-rejects-bad-platform" {} "touch $out";

    # mkChocoPackage returns expected attributes and package layout.
    mkChocoPackage-shape = let
      f = self.lib.mkChocoPackage {
        inherit pkgs;
        id = "modde";
        version = "1.2.3";
        description = "Cross-platform game mod manager for Windows";
        homepage = "https://modde.tartanoglu.com";
        license = "MIT";
        licenseUrl = "https://codeberg.org/caniko/modde/raw/branch/main/LICENSE";
        authors = ["Can Tartanoglu"];
        architectures.x64 = {
          url = "https://codeberg.org/caniko/modde/releases/download/1.2.3/modde-1.2.3-x86_64-pc-windows-msvc.zip";
          sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        };
        binaries = ["modde"];
      };
    in
      assert f ? nuspecText;
      assert f ? installScriptText;
      assert f ? packageDir;
      assert f ? nupkgPath;
      assert f ? artifactBuilder;
      assert f.artifactBuilder.kind == "chocolatey-builder";
      assert f.artifactBuilder.output == toString f.nupkgPath;
      assert builtins.isString f.nuspecText;
      assert builtins.isString f.installScriptText;
      assert pkgs.lib.hasInfix "<id>modde</id>" f.nuspecText;
      assert pkgs.lib.hasInfix "<version>1.2.3</version>" f.nuspecText;
      assert pkgs.lib.hasInfix "url64bit" f.installScriptText;
      assert pkgs.lib.hasInfix "checksum64" f.installScriptText;
        pkgs.runCommand "check-mkChocoPackage-shape" {
          nativeBuildInputs = [pkgs.libxml2 pkgs.unzip];
        } ''
          test -f ${f.packageDir}/modde.nuspec
          test -f ${f.packageDir}/tools/chocolateyInstall.ps1
          test -f ${f.nupkgPath}
          grep '<id>modde</id>' ${f.packageDir}/modde.nuspec
          grep "url64bit = 'https://codeberg.org/caniko/modde/releases/download/1.2.3/modde-1.2.3-x86_64-pc-windows-msvc.zip'" \
            ${f.packageDir}/tools/chocolateyInstall.ps1
          xmllint --noout ${f.packageDir}/modde.nuspec
          unzip -l ${f.nupkgPath} | grep 'modde.nuspec'
          unzip -l ${f.nupkgPath} | grep 'tools/chocolateyInstall.ps1'
          touch $out
        '';

    # mkChocoTestEnvironment renders the Chocolatey Vagrant test environment.
    mkChocoTestEnvironment-render = let
      choco = self.lib.mkChocoPackage {
        inherit pkgs;
        id = "demo";
        version = "1.0.0";
        description = "Windows CLI package";
        homepage = "https://example.com";
        license = "MIT";
        licenseUrl = "https://example.com/LICENSE";
        authors = ["Example Author"];
        architectures.x64 = {
          url = "https://example.com/demo-1.0.0-x86_64-pc-windows-msvc.zip";
          sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        };
      };
      env = self.lib.mkChocoTestEnvironment {
        inherit pkgs;
        chocoPackage = choco;
        verifyPowerShell = ["demo --version"];
      };
    in
      assert env ? plan;
      assert env ? runner;
      assert env ? app;
      assert env ? environmentDir;
      assert env ? artifactBuilder;
      assert env ? runnerBuilder;
      assert env.artifactBuilder.output == toString choco.nupkgPath;
      assert env.runnerBuilder.environment == toString env.environmentDir;
      assert env.plan.kind == "chocolatey-vagrant";
      assert env.plan.hierarchy == ["generic-builder" "windows-builder" "chocolatey-builder" "generic" "windows" "chocolatey-vagrant"];
        pkgs.runCommand "check-mkChocoTestEnvironment-render" {} ''
          test -x ${env.runner}/bin/package-test-demo
          test -f ${env.environmentDir}/Vagrantfile
          test -f ${env.environmentDir}/packages/demo.1.0.0.nupkg
          grep -q 'chocolatey/test-environment' ${env.environmentDir}/Vagrantfile
          grep -q 'config.vm.synced_folder "packages", "/packages"' ${env.environmentDir}/Vagrantfile
          grep -q 'vagrant up --provider=virtualbox' ${env.runner}/bin/package-test-demo
          grep -q 'choco install demo --version 1.0.0 --source C:\\packages' ${env.environmentDir}/Vagrantfile
          touch $out
        '';

    # mkPackageTestPlan covers all package-helper families and marks unsupported runners explicitly.
    mkPackageTestPlan-adapters = let
      mk = kind:
        self.lib.mkPackageArtifactBuilder {
          inherit pkgs kind;
          packageName = "demo";
          version = "1.0.0";
          output = "${pkgs.emptyDirectory}/demo-${kind}";
          unsupportedBuilderReason = null;
        };
      builders = {
        appimage = mk "appimage";
        flatpak = mk "flatpak";
        copr-rpm = mk "copr-rpm";
        debian = mk "debian";
        homebrew = mk "homebrew";
        scoop = mk "scoop";
        chocolatey = mk "chocolatey";
      };
      plans =
        builtins.mapAttrs
        (kind: artifactBuilder:
          self.lib.mkPackageTestPlan {
            inherit pkgs kind artifactBuilder;
            installCommand = "install demo-${kind}";
            unsupportedRunnerReason = "No ${kind} package-test runner is implemented yet.";
          })
        builders;
    in
      assert builders.appimage.kind == "appimage-builder";
      assert builders.scoop.hierarchy == ["generic-builder" "windows-builder" "scoop-builder"];
      assert plans.appimage.kind == "appimage";
      assert plans.flatpak.kind == "flatpak";
      assert plans.copr-rpm.kind == "copr-rpm";
      assert plans.debian.kind == "debian";
      assert plans.homebrew.kind == "homebrew";
      assert plans.scoop.hierarchy == ["generic-builder" "windows-builder" "scoop-builder" "generic" "windows" "scoop"];
      assert plans.chocolatey.hierarchy == ["generic-builder" "windows-builder" "chocolatey-builder" "generic" "windows" "chocolatey"];
      assert plans.debian.unsupportedRunnerReason != null;
        pkgs.runCommand "check-mkPackageTestPlan-adapters" {} "touch $out";

    # mkChocoPackage emits the expected PowerShell keys for multiple Windows architectures.
    mkChocoPackage-multi-arch = let
      f = self.lib.mkChocoPackage {
        inherit pkgs;
        id = "multi-app";
        version = "2.0.0";
        description = "Windows CLI package";
        homepage = "https://example.com";
        license = "Apache-2.0";
        licenseUrl = "https://example.com/LICENSE";
        authors = ["Example Author"];
        architectures = {
          x64 = {
            url = "https://example.com/multi-app-2.0.0-x86_64-pc-windows-msvc.zip";
            sha256 = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
          };
          arm64 = {
            url = "https://example.com/multi-app-2.0.0-aarch64-pc-windows-msvc.zip";
            sha256 = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
          };
        };
        binaries = ["multi-app"];
      };
    in
      assert pkgs.lib.hasInfix "url64bit" f.installScriptText;
      assert pkgs.lib.hasInfix "urlArm64" f.installScriptText;
      assert pkgs.lib.hasInfix "checksumType64 = 'sha256'" f.installScriptText;
      assert pkgs.lib.hasInfix "checksumTypeArm64 = 'sha256'" f.installScriptText;
        pkgs.runCommand "check-mkChocoPackage-multi-arch" {} "touch $out";

    # mkChocoPackage omits checksum args when the caller uses :no_check placeholders.
    mkChocoPackage-no-check-placeholder = let
      f = self.lib.mkChocoPackage {
        inherit pkgs;
        id = "placeholder-app";
        version = "1.0.0";
        description = "Placeholder checksum package";
        homepage = "https://example.com";
        license = "MIT";
        licenseUrl = "https://example.com/LICENSE";
        authors = ["Example Author"];
        architectures.x64 = {
          url = "https://example.com/placeholder-app-1.0.0-x86_64-pc-windows-msvc.zip";
          sha256 = ":no_check";
        };
        binaries = ["placeholder-app"];
      };
    in
      assert pkgs.lib.hasInfix "url64bit" f.installScriptText;
      assert !(pkgs.lib.hasInfix "checksum64" f.installScriptText);
      assert !(pkgs.lib.hasInfix "checksumType64" f.installScriptText);
        pkgs.runCommand "check-mkChocoPackage-no-check-placeholder" {
          nativeBuildInputs = [pkgs.libxml2];
        } ''
          test -f ${f.packageDir}/placeholder-app.nuspec
          grep "url64bit = 'https://example.com/placeholder-app-1.0.0-x86_64-pc-windows-msvc.zip'" \
            ${f.packageDir}/tools/chocolateyInstall.ps1
          if grep -q "checksum64" ${f.packageDir}/tools/chocolateyInstall.ps1; then
            echo "checksum64 should be omitted for :no_check" >&2
            exit 1
          fi
          xmllint --noout ${f.packageDir}/placeholder-app.nuspec
          touch $out
        '';

    # mkChocoPackage XML-escapes metadata content before writing the nuspec.
    mkChocoPackage-xml-escaping = let
      f = self.lib.mkChocoPackage {
        inherit pkgs;
        id = "xml-app";
        version = "1.0.0";
        description = ''Needs <escaping> & "quotes" in XML'';
        homepage = "https://example.com";
        license = "MIT";
        licenseUrl = "https://example.com/LICENSE";
        authors = ["Example Author"];
        architectures.x86 = {
          url = "https://example.com/xml-app-1.0.0-i686-pc-windows-msvc.zip";
          sha256 = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
        };
        binaries = ["xml-app"];
      };
    in
      assert pkgs.lib.hasInfix "&lt;escaping&gt;" f.nuspecText;
      assert pkgs.lib.hasInfix "&amp;" f.nuspecText;
      assert pkgs.lib.hasInfix "&quot;quotes&quot;" f.nuspecText;
        pkgs.runCommand "check-mkChocoPackage-xml-escaping" {
          nativeBuildInputs = [pkgs.libxml2];
        } ''
          xmllint --noout ${f.packageDir}/xml-app.nuspec
          touch $out
        '';

    # mkChocoPackage rejects a Chocolatey-illegal package id.
    mkChocoPackage-rejects-bad-id = let
      result = builtins.tryEval (self.lib.mkChocoPackage {
        inherit pkgs;
        id = "Bad_Id";
        version = "1.0.0";
        description = "Bad id package";
        homepage = "https://example.com";
        license = "MIT";
        licenseUrl = "https://example.com/LICENSE";
        authors = ["Example Author"];
        architectures.x64 = {
          url = "https://example.com/bad-id.zip";
          sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        };
        binaries = ["bad-id"];
      });
    in
      assert !result.success;
        pkgs.runCommand "check-mkChocoPackage-rejects-bad-id" {} "touch $out";

    # mkChocoPackage rejects versions with a leading v.
    mkChocoPackage-rejects-bad-version = let
      result = builtins.tryEval (self.lib.mkChocoPackage {
        inherit pkgs;
        id = "bad-version";
        version = "v1.0.0";
        description = "Bad version package";
        homepage = "https://example.com";
        license = "MIT";
        licenseUrl = "https://example.com/LICENSE";
        authors = ["Example Author"];
        architectures.x64 = {
          url = "https://example.com/bad-version.zip";
          sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        };
        binaries = ["bad-version"];
      });
    in
      assert !result.success;
        pkgs.runCommand "check-mkChocoPackage-rejects-bad-version" {} "touch $out";

    # mkChocoPackage rejects descriptions longer than Chocolatey's nuspec limit.
    mkChocoPackage-rejects-long-description = let
      result = builtins.tryEval (self.lib.mkChocoPackage {
        inherit pkgs;
        id = "long-description";
        version = "1.0.0";
        description = builtins.concatStringsSep "" (builtins.genList (_: "a") 4001);
        homepage = "https://example.com";
        license = "MIT";
        licenseUrl = "https://example.com/LICENSE";
        authors = ["Example Author"];
        architectures.x64 = {
          url = "https://example.com/long-description.zip";
          sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        };
        binaries = ["long-description"];
      });
    in
      assert !result.success;
        pkgs.runCommand "check-mkChocoPackage-rejects-long-description" {} "touch $out";

    # mkChocoPackage rejects non-HTTPS homepages.
    mkChocoPackage-rejects-http-homepage = let
      result = builtins.tryEval (self.lib.mkChocoPackage {
        inherit pkgs;
        id = "http-homepage";
        version = "1.0.0";
        description = "HTTP homepage package";
        homepage = "http://example.com";
        license = "MIT";
        licenseUrl = "https://example.com/LICENSE";
        authors = ["Example Author"];
        architectures.x64 = {
          url = "https://example.com/http-homepage.zip";
          sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        };
        binaries = ["http-homepage"];
      });
    in
      assert !result.success;
        pkgs.runCommand "check-mkChocoPackage-rejects-http-homepage" {} "touch $out";

    # mkChocoPackage rejects unsupported Windows architecture keys.
    mkChocoPackage-rejects-bad-arch-key = let
      result = builtins.tryEval (self.lib.mkChocoPackage {
        inherit pkgs;
        id = "bad-arch";
        version = "1.0.0";
        description = "Bad architecture package";
        homepage = "https://example.com";
        license = "MIT";
        licenseUrl = "https://example.com/LICENSE";
        authors = ["Example Author"];
        architectures.ppc64 = {
          url = "https://example.com/bad-arch.zip";
          sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        };
        binaries = ["bad-arch"];
      });
    in
      assert !result.success;
        pkgs.runCommand "check-mkChocoPackage-rejects-bad-arch-key" {} "touch $out";

    # mkScoopManifest returns expected attributes and a bin list.
    mkScoopManifest-shape = let
      m = self.lib.mkScoopManifest {
        inherit pkgs;
        name = "modde";
        version = "1.2.3";
        description = "Cross-platform game mod manager";
        homepage = "https://modde.tartanoglu.com";
        license = "MIT";
        architectures."64bit" = {
          url = "https://example.com/modde-1.2.3-x86_64-pc-windows-msvc.zip";
          sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        };
        binaries = ["modde.exe" "modde-ui.exe"];
      };
      parsed = builtins.fromJSON m.manifestText;
    in
      assert m ? manifestText;
      assert m ? manifestPath;
      assert m ? artifactBuilder;
      assert m.artifactBuilder.kind == "scoop-builder";
      assert m.artifactBuilder.output == toString m.manifestPath;
      assert builtins.isString m.manifestText;
      assert parsed.architecture."64bit".url == "https://example.com/modde-1.2.3-x86_64-pc-windows-msvc.zip";
      assert parsed.bin == ["modde.exe" "modde-ui.exe"];
        pkgs.runCommand "check-mkScoopManifest-shape" {
          nativeBuildInputs = [pkgs.jq];
        } ''
          jq -e '.architecture["64bit"].url == "https://example.com/modde-1.2.3-x86_64-pc-windows-msvc.zip"' ${m.manifestPath}
          jq -e '.bin == ["modde.exe", "modde-ui.exe"]' ${m.manifestPath}
          touch $out
        '';

    # mkScoopManifest emits the requested architecture blocks.
    mkScoopManifest-multi-arch = let
      both = self.lib.mkScoopManifest {
        inherit pkgs;
        name = "multi-app";
        version = "1.0.0";
        description = "Multi arch app";
        homepage = "https://example.com";
        license = "Apache-2.0";
        architectures = {
          "64bit" = {
            url = "https://example.com/multi-app-1.0.0-x86_64-pc-windows-msvc.zip";
            sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
          };
          arm64 = {
            url = "https://example.com/multi-app-1.0.0-aarch64-pc-windows-msvc.zip";
            sha256 = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
          };
        };
      };
      armOnly = self.lib.mkScoopManifest {
        inherit pkgs;
        name = "arm-app";
        version = "1.0.0";
        description = "Arm app";
        homepage = "https://example.com";
        license = "MIT";
        architectures.arm64 = {
          url = "https://example.com/arm-app-1.0.0-aarch64-pc-windows-msvc.zip";
          sha256 = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
        };
      };
      bothParsed = builtins.fromJSON both.manifestText;
      armOnlyParsed = builtins.fromJSON armOnly.manifestText;
    in
      assert bothParsed.architecture ? "64bit";
      assert bothParsed.architecture ? arm64;
      assert armOnlyParsed.architecture ? arm64;
      assert !(armOnlyParsed.architecture ? "64bit");
        pkgs.runCommand "check-mkScoopManifest-multi-arch" {
          nativeBuildInputs = [pkgs.jq];
        } ''
          jq -e '.architecture["64bit"].hash == "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"' ${both.manifestPath}
          jq -e '.architecture.arm64.hash == "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"' ${both.manifestPath}
          jq -e '.architecture.arm64.url == "https://example.com/arm-app-1.0.0-aarch64-pc-windows-msvc.zip"' ${armOnly.manifestPath}
          touch $out
        '';

    # mkScoopManifest omits hash entries for :no_check placeholders.
    mkScoopManifest-no-check-placeholder = let
      m = self.lib.mkScoopManifest {
        inherit pkgs;
        name = "placeholder-app";
        version = "1.0.0";
        description = "Placeholder checksum app";
        homepage = "https://example.com";
        license = "MIT";
        architectures."64bit" = {
          url = "https://example.com/placeholder-app-1.0.0-x86_64-pc-windows-msvc.zip";
          sha256 = ":no_check";
        };
      };
      parsed = builtins.fromJSON m.manifestText;
    in
      assert parsed ? _comment;
      assert !(parsed.architecture."64bit" ? hash);
        pkgs.runCommand "check-mkScoopManifest-no-check-placeholder" {
          nativeBuildInputs = [pkgs.jq];
        } ''
          jq -e '._comment | contains(":no_check")' ${m.manifestPath}
          jq -e 'has("architecture") and (.architecture["64bit"] | has("hash") | not)' ${m.manifestPath}
          touch $out
        '';

    # mkScoopManifest rejects a Scoop-illegal manifest name.
    mkScoopManifest-rejects-bad-name = let
      result = builtins.tryEval (self.lib.mkScoopManifest {
        inherit pkgs;
        name = "Bad_Name";
        version = "1.0.0";
        description = "Bad name app";
        homepage = "https://example.com";
        license = "MIT";
        architectures."64bit" = {
          url = "https://example.com/bad-name.zip";
          sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        };
      });
    in
      assert !result.success;
        pkgs.runCommand "check-mkScoopManifest-rejects-bad-name" {} "touch $out";

    # mkScoopManifest rejects versions with a leading v.
    mkScoopManifest-rejects-bad-version = let
      result = builtins.tryEval (self.lib.mkScoopManifest {
        inherit pkgs;
        name = "bad-version";
        version = "v1.0.0";
        description = "Bad version app";
        homepage = "https://example.com";
        license = "MIT";
        architectures."64bit" = {
          url = "https://example.com/bad-version.zip";
          sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        };
      });
    in
      assert !result.success;
        pkgs.runCommand "check-mkScoopManifest-rejects-bad-version" {} "touch $out";

    # mkScoopManifest rejects descriptions longer than the helper allows.
    mkScoopManifest-rejects-long-desc = let
      result = builtins.tryEval (self.lib.mkScoopManifest {
        inherit pkgs;
        name = "long-desc";
        version = "1.0.0";
        description = "This description is deliberately longer than eighty characters so validation rejects it.";
        homepage = "https://example.com";
        license = "MIT";
        architectures."64bit" = {
          url = "https://example.com/long-desc.zip";
          sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        };
      });
    in
      assert !result.success;
        pkgs.runCommand "check-mkScoopManifest-rejects-long-desc" {} "touch $out";

    # mkScoopManifest rejects non-HTTPS homepages.
    mkScoopManifest-rejects-http-homepage = let
      result = builtins.tryEval (self.lib.mkScoopManifest {
        inherit pkgs;
        name = "http-homepage";
        version = "1.0.0";
        description = "HTTP homepage app";
        homepage = "http://example.com";
        license = "MIT";
        architectures."64bit" = {
          url = "https://example.com/http-homepage.zip";
          sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        };
      });
    in
      assert !result.success;
        pkgs.runCommand "check-mkScoopManifest-rejects-http-homepage" {} "touch $out";

    # mkScoopManifest rejects unsupported architecture keys.
    mkScoopManifest-rejects-bad-arch-key = let
      result = builtins.tryEval (self.lib.mkScoopManifest {
        inherit pkgs;
        name = "bad-arch";
        version = "1.0.0";
        description = "Bad architecture app";
        homepage = "https://example.com";
        license = "MIT";
        architectures.x86_64 = {
          url = "https://example.com/bad-arch.zip";
          sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        };
      });
    in
      assert !result.success;
        pkgs.runCommand "check-mkScoopManifest-rejects-bad-arch-key" {} "touch $out";
  }
  // pkgs.lib.optionalAttrs isLinux {
    # mkAppImage exposes the generated AppImage through the builder hierarchy.
    mkAppImage-shape = let
      fakeNixAppImage = {
        lib.${system}.mkAppImage = args:
          pkgs.runCommand "${args.pname or "demo"}.AppImage" {} "touch $out";
      };
      image = self.lib.mkAppImage {
        inherit system;
        nix-appimage = fakeNixAppImage;
        program = "${pkgs.hello}/bin/hello";
        pname = "hello-app";
        version = "1.0.0";
      };
    in
      assert image ? artifactBuilder;
      assert image.artifactBuilder.kind == "appimage-builder";
      assert image.artifactBuilder.output == toString image;
        pkgs.runCommand "check-mkAppImage-shape" {} "touch $out";

    # mkFlatpakManifest returns expected attributes
    mkFlatpakManifest-shape = let
      m = self.lib.mkFlatpakManifest {
        inherit pkgs;
        appId = "com.example.TestApp";
        pname = "test-app";
        desktopFile = "[Desktop Entry]\nType=Application\nName=Test\nExec=test-app";
      };
    in
      assert m ? manifestText;
      assert m ? manifestPath;
      assert m ? artifactBuilder;
      assert m.artifactBuilder.kind == "flatpak-builder";
      assert m.artifactBuilder.output == toString m.manifestPath;
      assert builtins.isString m.manifestText;
      assert pkgs.lib.hasInfix "com.example.TestApp" m.manifestText;
      assert pkgs.lib.hasInfix "test-app" m.manifestText;
      assert pkgs.lib.hasInfix "org.freedesktop.Platform" m.manifestText;
      assert pkgs.lib.hasInfix "24.08" m.manifestText;
        pkgs.runCommand "check-mkFlatpakManifest-shape" {} "touch $out";

    # mkFlatpakManifest respects custom runtime
    mkFlatpakManifest-custom-runtime = let
      m = self.lib.mkFlatpakManifest {
        inherit pkgs;
        appId = "org.test.App";
        pname = "myapp";
        desktopFile = "[Desktop Entry]\nType=Application\nName=Test\nExec=myapp";
        runtime = "org.gnome.Platform";
        runtimeVersion = "46";
        sdk = "org.gnome.Sdk";
      };
    in
      assert pkgs.lib.hasInfix "org.gnome.Platform" m.manifestText;
      assert pkgs.lib.hasInfix "46" m.manifestText;
      assert pkgs.lib.hasInfix "org.gnome.Sdk" m.manifestText;
        pkgs.runCommand "check-mkFlatpakManifest-custom-runtime" {} "touch $out";

    # mkFlatpakManifest respects custom finishArgs
    mkFlatpakManifest-finish-args = let
      m = self.lib.mkFlatpakManifest {
        inherit pkgs;
        appId = "org.test.App2";
        pname = "myapp2";
        desktopFile = "[Desktop Entry]\nType=Application\nName=Test\nExec=myapp2";
        finishArgs = ["--share=network" "--socket=x11"];
      };
    in
      assert pkgs.lib.hasInfix "share=network" m.manifestText;
      assert pkgs.lib.hasInfix "socket=x11" m.manifestText;
        pkgs.runCommand "check-mkFlatpakManifest-finish-args" {} "touch $out";

    mkMinisignSign-shape = let
      app = self.lib.mkMinisignSign {
        inherit pkgs;
        files = ["release/SHA256SUMS.txt"];
      };
    in
      assert app.type == "app";
        pkgs.runCommand "check-mkMinisignSign-shape" {} ''
          cp ${app.program} script
          grep 'minisign -S -s "$keyfile"' script
          grep 'MINISIGN_SECRET_KEY' script
          grep 'release/SHA256SUMS.txt' script
          touch $out
        '';

    mkMinisignVerify-shape = let
      app = self.lib.mkMinisignVerify {
        inherit pkgs;
        files = ["release/SHA256SUMS.txt"];
      };
    in
      assert app.type == "app";
        pkgs.runCommand "check-mkMinisignVerify-shape" {} ''
          cp ${app.program} script
          grep 'minisign -V -p' script
          grep 'keys/minisign.pub' script
          touch $out
        '';

    # mkSccacheEnv produces expected env vars
    mkSccacheEnv-shape = let
      env = self.lib.mkSccacheEnv.mkSccacheEnv {
        bucket = "sccache";
        endpoint = "http://127.0.0.1:3900";
      };
    in
      assert env ? SCCACHE_BUCKET;
      assert env ? SCCACHE_ENDPOINT;
      assert env ? SCCACHE_REGION;
      assert env ? SCCACHE_S3_USE_SSL;
      assert env.SCCACHE_BUCKET == "sccache";
      assert env.SCCACHE_ENDPOINT == "http://127.0.0.1:3900";
      assert env.SCCACHE_S3_USE_SSL == "false";
        pkgs.runCommand "check-mkSccacheEnv-shape" {} "touch $out";

    # mkSccacheEnv with SSL sets SCCACHE_S3_USE_SSL=true
    mkSccacheEnv-ssl = let
      env = self.lib.mkSccacheEnv.mkSccacheEnv {
        bucket = "sccache";
        endpoint = "https://s3.example.com";
        useSsl = true;
      };
    in
      assert env.SCCACHE_S3_USE_SSL == "true";
        pkgs.runCommand "check-mkSccacheEnv-ssl" {} "touch $out";

    # mkSccacheEnv with keyPrefix sets SCCACHE_S3_KEY_PREFIX
    mkSccacheEnv-prefix = let
      env = self.lib.mkSccacheEnv.mkSccacheEnv {
        bucket = "sccache";
        endpoint = "http://127.0.0.1:3900";
        keyPrefix = "atlas";
      };
    in
      assert env.SCCACHE_S3_KEY_PREFIX == "atlas";
        pkgs.runCommand "check-mkSccacheEnv-prefix" {} "touch $out";

    # mkSccacheCraneEnv returns env vars when enabled with S3 params
    sccache-crane-env-shape = let
      env = self.lib.mkSccacheCraneEnv {
        enable = true;
        package = "/run/current-system/sw/bin/sccache";
        bucket = "sccache";
        endpoint = "http://127.0.0.1:3900";
        accessKeyId = "GKsomekey";
        secretAccessKey = "somesecret";
      };
    in
      assert env ? RUSTC_WRAPPER;
      assert env ? SCCACHE_BUCKET;
      assert env ? AWS_ACCESS_KEY_ID;
      assert env ? SCCACHE_CONNECT_TIMEOUT;
      assert env.RUSTC_WRAPPER == "/run/current-system/sw/bin/sccache";
      assert env.SCCACHE_BUCKET == "sccache";
      assert env.AWS_ACCESS_KEY_ID == "GKsomekey";
      assert env.SCCACHE_CONNECT_TIMEOUT == "2";
        pkgs.runCommand "check-sccache-crane-env-shape" {} "touch $out";

    # mkSccacheCraneEnv returns {} when disabled
    sccache-crane-env-disabled = let
      env = self.lib.mkSccacheCraneEnv {enable = false;};
    in
      assert env == {};
        pkgs.runCommand "check-sccache-crane-env-disabled" {} "touch $out";

    # mkSccacheCraneEnv returns local-only env when S3 params missing
    sccache-crane-env-no-s3 = let
      env = self.lib.mkSccacheCraneEnv {
        enable = true;
        package = "${pkgs.sccache}/bin/sccache";
      };
    in
      assert env ? RUSTC_WRAPPER;
      assert env ? SCCACHE_DIR;
      assert env ? XDG_CACHE_HOME;
      assert !(env ? SCCACHE_BUCKET);
      assert env.RUSTC_WRAPPER == "${pkgs.sccache}/bin/sccache";
      assert env.SCCACHE_DIR == "$NIX_BUILD_TOP/.sccache";
      assert env.XDG_CACHE_HOME == "$NIX_BUILD_TOP/.sccache";
        pkgs.runCommand "check-sccache-crane-env-no-s3" {} "touch $out";

    # mkSccacheCraneEnv returns daemon-mode env when daemonUds is set
    sccache-crane-env-daemon-uds = let
      env = self.lib.mkSccacheCraneEnv {
        enable = true;
        package = "${pkgs.sccache}/bin/sccache";
        daemonUds = "/run/sccache/sock";
      };
    in
      assert env ? RUSTC_WRAPPER;
      assert env ? SCCACHE_SERVER_UDS;
      assert env ? SCCACHE_CONNECT_TIMEOUT;
      assert !(env ? SCCACHE_DIR);
      assert !(env ? XDG_CACHE_HOME);
      assert env.RUSTC_WRAPPER == "${pkgs.sccache}/bin/sccache";
      assert env.SCCACHE_SERVER_UDS == "/run/sccache/sock";
      assert env.SCCACHE_CONNECT_TIMEOUT == "2";
        pkgs.runCommand "check-sccache-crane-env-daemon-uds" {} "touch $out";

    # NixOS module exports envVars with credentials
    sccache-home-runtime-basedirs = let
      evaluated = pkgs.lib.evalModules {
        specialArgs = {
          inherit pkgs;
          osConfig = {
            users.users.can.uid = 1000;
            programs.harborRs.sccache = {
              enable = true;
              remoteEnvVars = {
                SCCACHE_BUCKET = "sccache";
                SCCACHE_ENDPOINT = "http://garage.test";
              };
            };
          };
        };
        modules = [
          self.homeManagerModules.sccache
          ({lib, ...}: {
            options = {
              assertions = lib.mkOption {type = lib.types.listOf lib.types.unspecified;};
              home.username = lib.mkOption {type = lib.types.str;};
              home.homeDirectory = lib.mkOption {type = lib.types.str;};
              home.packages = lib.mkOption {type = lib.types.listOf lib.types.package;};
              home.sessionVariables = lib.mkOption {type = lib.types.attrsOf lib.types.str;};
              programs.nushell.environmentVariables = lib.mkOption {type = lib.types.attrsOf lib.types.str;};
              systemd.user.services = lib.mkOption {type = lib.types.attrsOf lib.types.unspecified;};
              systemd.user.sessionVariables = lib.mkOption {type = lib.types.attrsOf lib.types.str;};
              xdg.cacheHome = lib.mkOption {type = lib.types.str;};
            };
            config = {
              assertions = [];
              home = {
                username = "can";
                homeDirectory = "/home/can";
                packages = [];
                sessionVariables = {};
              };
              programs = {
                nushell.environmentVariables = {};
                harborRs.sccache.userDaemon = {
                  enable = true;
                  basedirs = ["/build" "/workspace"];
                  basedirsFile = "/run/user/1000/canix/sccache-basedirs";
                  multiLevelChain = "disk,redis,s3";
                  redisEndpoint = "redis+unix://localhost/run/redis-sccache/redis.sock";
                  redisKeyPrefix = "canix/test-rust-v1-sccache-0.16.0";
                  redisRwMode = "READ_WRITE";
                  multiLevelWriteErrorPolicy = "ignore";
                  environment.SCCACHE_CACHE_SIZE = "10G";
                };
              };
              systemd.user = {
                services = {};
                sessionVariables = {};
              };
              xdg.cacheHome = "/home/can/.cache";
            };
          })
        ];
      };
      cfg = evaluated.config;
      service = cfg.systemd.user.services.sccache-user-daemon.Service;
    in
      assert builtins.elem "SCCACHE_BASEDIRS=/build:/workspace" service.Environment;
      assert builtins.elem "SCCACHE_MULTILEVEL_CHAIN=disk,redis,s3" service.Environment;
      assert builtins.elem "SCCACHE_REDIS_ENDPOINT=redis+unix://localhost/run/redis-sccache/redis.sock" service.Environment;
      assert builtins.elem "SCCACHE_REDIS_KEY_PREFIX=canix/test-rust-v1-sccache-0.16.0" service.Environment;
      assert builtins.elem "SCCACHE_REDIS_RW_MODE=READ_WRITE" service.Environment;
      assert builtins.elem "SCCACHE_MULTILEVEL_WRITE_ERROR_POLICY=ignore" service.Environment;
      assert builtins.elem "SCCACHE_CACHE_SIZE=10G" service.Environment;
      assert pkgs.lib.hasInfix "sccache-user-daemon-launcher" service.ExecStart;
      assert cfg.home.sessionVariables.CARGO_INCREMENTAL == "0";
      assert cfg.systemd.user.sessionVariables.CARGO_INCREMENTAL == "0";
        pkgs.runCommand "check-sccache-home-runtime-basedirs" {} "touch $out";

    sccache-module-env-vars = let
      evaluated = pkgs.lib.evalModules {
        modules = [
          self.nixosModules.sccache
          {
            options.environment = {
              systemPackages = pkgs.lib.mkOption {
                type = pkgs.lib.types.listOf pkgs.lib.types.package;
                default = [];
              };
              variables = pkgs.lib.mkOption {
                type = pkgs.lib.types.attrsOf pkgs.lib.types.str;
                default = {};
              };
            };
            options = {
              systemd = {
                tmpfiles.rules = pkgs.lib.mkOption {
                  type = pkgs.lib.types.listOf pkgs.lib.types.str;
                  default = [];
                };
                services = pkgs.lib.mkOption {
                  type = pkgs.lib.types.attrsOf pkgs.lib.types.raw;
                  default = {};
                };
              };
              nix.settings = pkgs.lib.mkOption {
                type = pkgs.lib.types.attrsOf pkgs.lib.types.raw;
                default = {};
              };
              users = {
                users = pkgs.lib.mkOption {
                  type = pkgs.lib.types.attrsOf pkgs.lib.types.raw;
                  default = {};
                };
                groups = pkgs.lib.mkOption {
                  type = pkgs.lib.types.attrsOf pkgs.lib.types.raw;
                  default = {};
                };
              };
              assertions = pkgs.lib.mkOption {
                type = pkgs.lib.types.listOf pkgs.lib.types.raw;
                default = [];
              };
              nixpkgs.overlays = pkgs.lib.mkOption {
                type = pkgs.lib.types.listOf pkgs.lib.types.raw;
                default = [];
              };
            };
          }
          {
            programs.harborRs.sccache = {
              enable = true;
              cacheEndpoint = "http://127.0.0.1:3900";
              cacheBucket = "sccache";
              accessKeyId = "GKkey";
              secretAccessKey = "secret";
            };
          }
        ];
        specialArgs = {inherit pkgs;};
      };
      env = evaluated.config.programs.harborRs.sccache.envVars;
      remote = evaluated.config.programs.harborRs.sccache.remoteEnvVars;
    in
      assert env ? RUSTC_WRAPPER;
      assert env ? SCCACHE_BUCKET;
      assert env ? AWS_ACCESS_KEY_ID;
      assert env ? SCCACHE_CONNECT_TIMEOUT;
      assert env.RUSTC_WRAPPER == "${pkgs.sccache}/bin/sccache";
      assert env.SCCACHE_BUCKET == "sccache";
      assert env.AWS_ACCESS_KEY_ID == "GKkey";
      assert env.AWS_SECRET_ACCESS_KEY == "secret";
      assert env.SCCACHE_CONNECT_TIMEOUT == "2";
      assert remote.SCCACHE_BUCKET == "sccache";
      assert remote.AWS_ACCESS_KEY_ID == "GKkey";
      assert !(remote ? SCCACHE_REDIS_ENDPOINT);
      assert !(remote ? SCCACHE_SERVER_UDS);
        pkgs.runCommand "check-sccache-module-env-vars" {} "touch $out";

    # NixOS module daemon shape — socket in RuntimeDirectory, not /tmp/sccache
    sccache-module-daemon-shape = let
      mockBase = {
        options = {
          environment = {
            systemPackages = pkgs.lib.mkOption {
              type = pkgs.lib.types.listOf pkgs.lib.types.package;
              default = [];
            };
            variables = pkgs.lib.mkOption {
              type = pkgs.lib.types.attrsOf pkgs.lib.types.str;
              default = {};
            };
          };
          systemd = {
            tmpfiles.rules = pkgs.lib.mkOption {
              type = pkgs.lib.types.listOf pkgs.lib.types.str;
              default = [];
            };
            services = pkgs.lib.mkOption {
              type = pkgs.lib.types.attrsOf pkgs.lib.types.raw;
              default = {};
            };
          };
          nix.settings = pkgs.lib.mkOption {
            type = pkgs.lib.types.attrsOf pkgs.lib.types.raw;
            default = {};
          };
          users = {
            users = pkgs.lib.mkOption {
              type = pkgs.lib.types.attrsOf pkgs.lib.types.raw;
              default = {};
            };
            groups = pkgs.lib.mkOption {
              type = pkgs.lib.types.attrsOf pkgs.lib.types.raw;
              default = {};
            };
          };
          assertions = pkgs.lib.mkOption {
            type = pkgs.lib.types.listOf pkgs.lib.types.raw;
            default = [];
          };
          nixpkgs.overlays = pkgs.lib.mkOption {
            type = pkgs.lib.types.listOf pkgs.lib.types.raw;
            default = [];
          };
        };
      };
      evaluated = pkgs.lib.evalModules {
        modules = [
          self.nixosModules.sccache
          mockBase
          {
            programs.harborRs.sccache = {
              enable = true;
              daemon.enable = true;
              cacheEndpoint = "http://127.0.0.1:3900";
              cacheBucket = "sccache";
              accessKeyId = "GKkey";
              secretAccessKey = "secret";
            };
          }
        ];
        specialArgs = {inherit pkgs;};
      };
      svc = evaluated.config.systemd.services.sccache-daemon;
      settings = evaluated.config.nix.settings;
    in
      assert !builtins.any (a: !a.assertion) evaluated.config.assertions;
      assert settings.extra-sandbox-paths == ["/run/sccache"];
      assert settings."impure-env" == ["SCCACHE_SERVER_UDS=/run/sccache/sock"];
      assert svc.serviceConfig.RuntimeDirectory == "sccache";
      assert svc.serviceConfig.RuntimeDirectoryMode == "0755";
      assert svc.serviceConfig.Type == "exec";
      assert svc.serviceConfig.Restart == "on-failure";
      assert svc.environment.SCCACHE_START_SERVER == "1";
      assert svc.environment.SCCACHE_NO_DAEMON == "1";
      assert builtins.any (entry: pkgs.lib.hasInfix "harbor-rs-sccache-wait-for-socket" entry) svc.serviceConfig.ExecStartPost;
        pkgs.runCommand "check-sccache-module-daemon-shape" {} "touch $out";

    # NixOS module can disable interactive global env without breaking daemon env
    sccache-module-global-env-disabled = let
      mockBase = {
        options = {
          environment = {
            systemPackages = pkgs.lib.mkOption {
              type = pkgs.lib.types.listOf pkgs.lib.types.package;
              default = [];
            };
            variables = pkgs.lib.mkOption {
              type = pkgs.lib.types.attrsOf pkgs.lib.types.str;
              default = {};
            };
          };
          systemd = {
            tmpfiles.rules = pkgs.lib.mkOption {
              type = pkgs.lib.types.listOf pkgs.lib.types.str;
              default = [];
            };
            services = pkgs.lib.mkOption {
              type = pkgs.lib.types.attrsOf pkgs.lib.types.raw;
              default = {};
            };
          };
          nix.settings = pkgs.lib.mkOption {
            type = pkgs.lib.types.attrsOf pkgs.lib.types.raw;
            default = {};
          };
          users = {
            users = pkgs.lib.mkOption {
              type = pkgs.lib.types.attrsOf pkgs.lib.types.raw;
              default = {};
            };
            groups = pkgs.lib.mkOption {
              type = pkgs.lib.types.attrsOf pkgs.lib.types.raw;
              default = {};
            };
          };
          assertions = pkgs.lib.mkOption {
            type = pkgs.lib.types.listOf pkgs.lib.types.raw;
            default = [];
          };
          nixpkgs.overlays = pkgs.lib.mkOption {
            type = pkgs.lib.types.listOf pkgs.lib.types.raw;
            default = [];
          };
        };
      };
      evaluated = pkgs.lib.evalModules {
        modules = [
          self.nixosModules.sccache
          mockBase
          {
            programs.harborRs.sccache = {
              enable = true;
              daemon.enable = true;
              setGlobalEnvironment = false;
              cacheEndpoint = "http://127.0.0.1:3900";
              cacheBucket = "sccache";
              accessKeyId = "GKkey";
              secretAccessKey = "secret";
            };
          }
        ];
        specialArgs = {inherit pkgs;};
      };
      env = evaluated.config.programs.harborRs.sccache.envVars;
      svc = evaluated.config.systemd.services.sccache-daemon;
      settings = evaluated.config.nix.settings;
    in
      assert evaluated.config.environment.variables == {};
      assert env.RUSTC_WRAPPER == "${pkgs.sccache}/bin/sccache";
      assert env.SCCACHE_SERVER_UDS == "/run/sccache/sock";
      assert env.AWS_ACCESS_KEY_ID == "GKkey";
      assert settings."impure-env" == ["SCCACHE_SERVER_UDS=/run/sccache/sock"];
      assert svc.environment.AWS_SECRET_ACCESS_KEY == "secret";
      assert svc.environment.SCCACHE_SERVER_UDS == "/run/sccache/sock";
        pkgs.runCommand "check-sccache-module-global-env-disabled" {} "touch $out";

    # NixOS module assertion fires when daemon + sandboxCacheDir coexist
    sccache-module-daemon-rejects-sandboxCacheDir = let
      mockBase = {
        options = {
          environment = {
            systemPackages = pkgs.lib.mkOption {
              type = pkgs.lib.types.listOf pkgs.lib.types.package;
              default = [];
            };
            variables = pkgs.lib.mkOption {
              type = pkgs.lib.types.attrsOf pkgs.lib.types.str;
              default = {};
            };
          };
          systemd = {
            tmpfiles.rules = pkgs.lib.mkOption {
              type = pkgs.lib.types.listOf pkgs.lib.types.str;
              default = [];
            };
            services = pkgs.lib.mkOption {
              type = pkgs.lib.types.attrsOf pkgs.lib.types.raw;
              default = {};
            };
          };
          nix.settings = pkgs.lib.mkOption {
            type = pkgs.lib.types.attrsOf pkgs.lib.types.raw;
            default = {};
          };
          users = {
            users = pkgs.lib.mkOption {
              type = pkgs.lib.types.attrsOf pkgs.lib.types.raw;
              default = {};
            };
            groups = pkgs.lib.mkOption {
              type = pkgs.lib.types.attrsOf pkgs.lib.types.raw;
              default = {};
            };
          };
          assertions = pkgs.lib.mkOption {
            type = pkgs.lib.types.listOf pkgs.lib.types.raw;
            default = [];
          };
          nixpkgs.overlays = pkgs.lib.mkOption {
            type = pkgs.lib.types.listOf pkgs.lib.types.raw;
            default = [];
          };
        };
      };
      evaluated = pkgs.lib.evalModules {
        modules = [
          self.nixosModules.sccache
          mockBase
          {
            programs.harborRs.sccache = {
              enable = true;
              daemon.enable = true;
              sandboxCacheDir = "/tmp/sccache";
              cacheEndpoint = "http://127.0.0.1:3900";
              cacheBucket = "sccache";
              accessKeyId = "GKkey";
              secretAccessKey = "secret";
            };
          }
        ];
        specialArgs = {inherit pkgs;};
      };
      assertions = evaluated.config.assertions;
    in
      assert builtins.any (a: !a.assertion) assertions;
        pkgs.runCommand "check-sccache-module-daemon-rejects-sandboxCacheDir" {} "touch $out";

    # crossbowPackages overlay injects sccache env into named packages
    sccache-module-crossbow-packages = let
      mockBase = {
        options = {
          environment = {
            systemPackages = pkgs.lib.mkOption {
              type = pkgs.lib.types.listOf pkgs.lib.types.package;
              default = [];
            };
            variables = pkgs.lib.mkOption {
              type = pkgs.lib.types.attrsOf pkgs.lib.types.str;
              default = {};
            };
          };
          systemd = {
            tmpfiles.rules = pkgs.lib.mkOption {
              type = pkgs.lib.types.listOf pkgs.lib.types.str;
              default = [];
            };
            services = pkgs.lib.mkOption {
              type = pkgs.lib.types.attrsOf pkgs.lib.types.raw;
              default = {};
            };
          };
          nix.settings = pkgs.lib.mkOption {
            type = pkgs.lib.types.attrsOf pkgs.lib.types.raw;
            default = {};
          };
          users = {
            users = pkgs.lib.mkOption {
              type = pkgs.lib.types.attrsOf pkgs.lib.types.raw;
              default = {};
            };
            groups = pkgs.lib.mkOption {
              type = pkgs.lib.types.attrsOf pkgs.lib.types.raw;
              default = {};
            };
          };
          assertions = pkgs.lib.mkOption {
            type = pkgs.lib.types.listOf pkgs.lib.types.raw;
            default = [];
          };
          nixpkgs.overlays = pkgs.lib.mkOption {
            type = pkgs.lib.types.listOf pkgs.lib.types.raw;
            default = [];
          };
        };
      };
      evaluated = pkgs.lib.evalModules {
        modules = [
          self.nixosModules.sccache
          mockBase
          {
            programs.harborRs.sccache = {
              enable = true;
              cacheEndpoint = "http://127.0.0.1:3900";
              cacheBucket = "sccache";
              accessKeyId = "GKkey";
              secretAccessKey = "secret";
              crossbowPackages = ["hello"];
            };
          }
        ];
        specialArgs = {inherit pkgs;};
      };
      # Apply the registered overlays to pkgs
      overlayedPkgs =
        pkgs.lib.foldl'
        (acc: overlay: acc.extend overlay)
        pkgs
        evaluated.config.nixpkgs.overlays;
      helloDrv = overlayedPkgs.hello;
      helloAttrs = (helloDrv.drvAttrs.env or {}) // helloDrv.drvAttrs;
    in
      assert helloAttrs ? RUSTC_WRAPPER;
      assert helloAttrs ? SCCACHE_BUCKET;
      assert helloAttrs ? SCCACHE_CONNECT_TIMEOUT;
        pkgs.runCommand "check-sccache-module-crossbow-packages" {} "touch $out";

    sccache-wrapRustPackageWithSccache-shapes = let
      envVars = {
        RUSTC_WRAPPER = "${pkgs.sccache}/bin/sccache";
        SCCACHE_BUCKET = "sccache";
        SCCACHE_CONNECT_TIMEOUT = "2";
      };
      plainWrapped = self.lib.wrapRustPackageWithSccache {
        package = pkgs.hello;
        sccachePackage = pkgs.sccache;
        inherit envVars;
      };
      envPackage = pkgs.hello.overrideAttrs (old: {
        env =
          (old.env or {})
          // {
            EXISTING_VAR = "kept";
          };
      });
      envWrapped = self.lib.wrapRustPackageWithSccache {
        package = envPackage;
        sccachePackage = pkgs.sccache;
        inherit envVars;
      };
      disabledWrapped = self.lib.wrapRustPackageWithSccache {
        package = pkgs.hello;
        sccachePackage = pkgs.sccache;
        inherit envVars;
        enable = false;
      };
      plainAttrs = (plainWrapped.drvAttrs.env or {}) // plainWrapped.drvAttrs;
      envAttrs = envWrapped.drvAttrs.env or {};
    in
      assert plainAttrs ? RUSTC_WRAPPER;
      assert plainAttrs ? SCCACHE_BUCKET;
      assert envAttrs.EXISTING_VAR == "kept";
      assert envAttrs.RUSTC_WRAPPER == envVars.RUSTC_WRAPPER;
      assert disabledWrapped == pkgs.hello;
        pkgs.runCommand "check-sccache-wrapRustPackageWithSccache-shapes" {} "touch $out";

    build-cache-policy-contract = let
      policy = self.lib.mkBuildCachePolicy {
        inherit pkgs;
        cacheRoot = "/tmp/sccache";
        namespaceScope = "test-rust";
        namespaceGeneration = 7;
      };
    in
      assert policy.contract.schemaVersion == 2;
      assert policy.contract.telemetrySchemaVersion == 1;
      assert policy.contract.telemetryMarker == "RS_HARBOR_SCCACHE_STATS_V1";
      assert policy.contract.namespace == "test-rust-v7-sccache-${pkgs.sccache.version}";
      assert policy.contract.sccacheVersion == pkgs.sccache.version;
      assert policy.contract.compiler == pkgs.buildPackages.rustc.version;
      assert policy.contract.rustToolchain.channel == "nightly-2026-09-15";
      assert policy.contract.redisSocketPath == "/run/redis-sccache/redis.sock";
      assert policy.sharedCacheDir == "/tmp/sccache/test-rust-v7-sccache-${pkgs.sccache.version}";
        pkgs.runCommand "check-build-cache-policy-contract" {} "touch $out";

    build-cache-policy-cross-platform-wrapper = let
      targetPkgs = pkgs.pkgsCross.aarch64-multiplatform;
      policy = self.lib.mkBuildCachePolicy {
        pkgs = targetPkgs;
        buildPackageSet = pkgs;
        sccachePackage = pkgs.sccache;
        cacheRoot = "/tmp/sccache";
      };
    in
      assert policy.wrapper.stdenv.buildPlatform.system == pkgs.stdenv.hostPlatform.system;
      assert policy.wrapper.stdenv.hostPlatform.system == pkgs.stdenv.hostPlatform.system;
        pkgs.runCommand "check-build-cache-policy-cross-platform-wrapper" {} "touch $out";

    build-cache-policy-builder-shapes = let
      policy = self.lib.mkBuildCachePolicy {inherit pkgs;};
      rust = policy.withRustCache {package = pkgs.hello;};
      dioxus = policy.withDioxusCache {package = pkgs.hello;};
      cmake = policy.withCmakeCache {package = pkgs.hello;};
      rustAttrs = (rust.drvAttrs.env or {}) // rust.drvAttrs;
      dioxusAttrs = (dioxus.drvAttrs.env or {}) // dioxus.drvAttrs;
      cmakeAttrs = cmake.drvAttrs.env or {};
    in
      assert rustAttrs.RUSTC_WRAPPER == policy.wrapperPath;
      assert rustAttrs.CARGO_INCREMENTAL == "0";
      assert rustAttrs.RS_HARBOR_SCCACHE_COMPILER == policy.contract.compiler;
      assert rustAttrs.RS_HARBOR_SCCACHE_TARGET_TRIPLE == pkgs.buildPackages.stdenv.buildPlatform.rust.cargoEnvVarTarget;
      assert rust.passthru.rsHarborBuildCacheWrapped;
      assert !(builtins.elem pkgs.sccache (rust.drvAttrs.nativeBuildInputs or []));
      assert dioxusAttrs.RUSTC_WRAPPER == policy.dioxusDispatcherPath;
      assert dioxusAttrs.DX_RUSTC_INNER_WRAPPER == policy.wrapperPath;
      assert dioxusAttrs.CARGO_INCREMENTAL == "0";
      assert cmakeAttrs.CMAKE_C_COMPILER_LAUNCHER != "";
      assert cmakeAttrs.CMAKE_CXX_COMPILER_LAUNCHER != "";
      assert !(builtins.elem pkgs.sccache (cmake.drvAttrs.nativeBuildInputs or []));
        pkgs.runCommand "check-build-cache-policy-builder-shapes" {} "touch $out";

    build-cache-policy-telemetry-fields = let
      policy = self.lib.mkBuildCachePolicy {inherit pkgs;};
      wrapped = policy.withRustCache {
        package = pkgs.hello;
        telemetry = {
          compiler = "rustc-test";
          targetTriple = "aarch64-unknown-linux-gnu";
        };
      };
      inferred = policy.withRustCache {
        package = pkgs.hello.overrideAttrs (_: {
          CARGO_BUILD_TARGET = "aarch64-unknown-linux-gnu";
        });
      };
      attrs = (wrapped.drvAttrs.env or {}) // wrapped.drvAttrs;
      inferredAttrs = (inferred.drvAttrs.env or {}) // inferred.drvAttrs;
      telemetryHooks =
        builtins.filter
        (input: pkgs.lib.hasPrefix "harbor-rs-sccache-telemetry-hook" (input.name or ""))
        (wrapped.drvAttrs.nativeBuildInputs or []);
      hook = builtins.head telemetryHooks;
    in
      assert attrs.RS_HARBOR_SCCACHE_COMPILER == "rustc-test";
      assert attrs.RS_HARBOR_SCCACHE_TARGET_TRIPLE == "aarch64-unknown-linux-gnu";
      assert inferredAttrs.RS_HARBOR_SCCACHE_TARGET_TRIPLE == "aarch64-unknown-linux-gnu";
        pkgs.runCommand "check-build-cache-policy-telemetry-fields" {} ''
          grep -F 'compiler: $compiler' ${hook}/nix-support/setup-hook >/dev/null
          grep -F 'targetTriple: $targetTriple' ${hook}/nix-support/setup-hook >/dev/null
          touch "$out"
        '';

    build-cache-policy-fail-closed = let
      policy = self.lib.mkBuildCachePolicy {inherit pkgs;};
    in
      pkgs.runCommand "check-build-cache-policy-fail-closed" {} ''
        ${pkgs.gnugrep}/bin/grep -F 'refusing an uncached build' ${policy.wrapperPath} >/dev/null
        ${pkgs.gnugrep}/bin/grep -F 'no managed cache transport is available' ${policy.wrapperPath} >/dev/null
        ${pkgs.gnugrep}/bin/grep -F 'host_cache_dir="$host_cache_root"/${policy.contract.namespace}' ${policy.wrapperPath} >/dev/null
        ${pkgs.gnugrep}/bin/grep -F 'managed server failed to become ready' ${policy.wrapperPath} >/dev/null
        if ${pkgs.gnugrep}/bin/grep -F 'exec "$compiler" "$@"' ${policy.wrapperPath} >/dev/null; then
          echo 'harbor-rs sandbox wrapper must not bypass sccache' >&2
          exit 1
        fi
        touch "$out"
      '';

    build-cache-policy-ephemeral-fallback = let
      withFallback = self.lib.mkBuildCachePolicy {
        inherit pkgs;
        ephemeralFallback = true;
      };
      withoutFallback = self.lib.mkBuildCachePolicy {inherit pkgs;};
      fallbackRoot = "$NIX_BUILD_TOP/harbor-rs-sccache-cache";
    in
      pkgs.runCommand "check-build-cache-policy-ephemeral-fallback" {} ''
        # Fallback executes: the wrapper starts a server on a private
        # build-scoped dir and sccache answers.
        ${withFallback.wrapperPath} --version >/dev/null
        test -d "${fallbackRoot}"
        mode=$(${pkgs.coreutils}/bin/stat -c %a "${fallbackRoot}")
        test "$mode" = "700"
        ${withFallback.wrapperPath} --stop-server >/dev/null 2>&1 || true
        # Default policy still refuses without a transport.
        if ${withoutFallback.wrapperPath} --version >/dev/null 2>&1; then
          echo 'default policy must refuse without a managed transport' >&2
          exit 1
        fi
        # A pre-existing symlink at the fallback path is refused.
        ${pkgs.coreutils}/bin/rm -rf "${fallbackRoot}"
        ${pkgs.coreutils}/bin/ln -s /tmp "${fallbackRoot}"
        if ${withFallback.wrapperPath} --version >/dev/null 2>&1; then
          echo 'fallback must refuse a symlinked cache path' >&2
          exit 1
        fi
        ${pkgs.coreutils}/bin/rm -f "${fallbackRoot}"
        touch "$out"
      '';

    build-cache-policy-concurrent-start = let
      policy = self.lib.mkBuildCachePolicy {
        inherit pkgs;
        cacheRoot = "/build/cache";
      };
    in
      pkgs.runCommand "check-build-cache-policy-concurrent-start" {} ''
        mkdir -m 0770 /build/cache
        ${policy.wrapperPath} --zero-stats &
        first=$!
        ${policy.wrapperPath} --zero-stats &
        second=$!
        wait "$first"
        wait "$second"
        ${policy.wrapperPath} --stop-server >/dev/null
        touch "$out"
      '';

    build-cache-policy-cross-shape = let
      policy = self.lib.mkBuildCachePolicy {
        inherit pkgs;
        buildPackageSet = pkgs.buildPackages;
      };
      linked = policy.withCrossLinker {
        package = pkgs.hello;
        buildPackageSet' = pkgs.buildPackages;
      };
      wrapped = policy.withRustCache {
        package = linked;
        linkerPackageSet = pkgs.buildPackages;
      };
      attrs = (wrapped.drvAttrs.env or {}) // wrapped.drvAttrs;
      target = pkgs.buildPackages.stdenv.buildPlatform.rust.cargoEnvVarTarget;
      targetUpper = pkgs.lib.toUpper (pkgs.lib.replaceStrings ["-"] ["_"] target);
      cacheWrappers =
        builtins.filter
        (input: input.rsHarborSandboxLocalSccache or false)
        (wrapped.drvAttrs.nativeBuildInputs or []);
      telemetryHooks =
        builtins.filter
        (input: pkgs.lib.hasPrefix "harbor-rs-sccache-telemetry-hook" (input.name or ""))
        (wrapped.drvAttrs.nativeBuildInputs or []);
    in
      assert !(linked.rsHarborBuildCacheWrapped or false);
      assert attrs.RUSTC_WRAPPER == policy.wrapperPath;
      assert attrs ? "CARGO_TARGET_${targetUpper}_LINKER";
      assert attrs."CARGO_TARGET_${targetUpper}_LINKER" == "${pkgs.buildPackages.stdenv.cc}/bin/cc";
      assert builtins.length cacheWrappers == 1;
      assert builtins.length telemetryHooks == 1;
        pkgs.runCommand "check-build-cache-policy-cross-shape" {} "touch $out";

    build-cache-policy-cross-shape-overrides-direct-linker = let
      policy = self.lib.mkBuildCachePolicy {inherit pkgs;};
      target = pkgs.buildPackages.stdenv.buildPlatform.rust.cargoEnvVarTarget;
      targetUpper = pkgs.lib.toUpper (pkgs.lib.replaceStrings ["-"] ["_"] target);
      direct = pkgs.hello.overrideAttrs (_: {
        "__CRANE_EXPORT_CARGO_TARGET_${targetUpper}_LINKER" = "cc";
      });
      wrapped = policy.withCrossRust {
        package = direct;
        buildPackageSet' = pkgs.buildPackages;
      };
      attrs = (wrapped.drvAttrs.env or {}) // wrapped.drvAttrs;
    in
      assert attrs."__CRANE_EXPORT_CARGO_TARGET_${targetUpper}_LINKER" == "${pkgs.buildPackages.stdenv.cc}/bin/cc";
        pkgs.runCommand "check-build-cache-policy-cross-shape-overrides-direct-linker" {} "touch $out";

    # Direct crane arguments such as RUSTC_WRAPPER must remain compatible with
    # the policy wrapper. Nix rejects duplicating those names inside `env`.
    build-cache-policy-preserves-direct-env = let
      policy = self.lib.mkBuildCachePolicy {inherit pkgs;};
      direct = pkgs.hello.overrideAttrs (_: {
        RUSTC_WRAPPER = "/direct/sccache";
        CARGO_INCREMENTAL = "0";
      });
      wrapped = policy.withRustCache {package = direct;};
      attrs = (wrapped.drvAttrs.env or {}) // wrapped.drvAttrs;
    in
      assert attrs.RUSTC_WRAPPER == "/direct/sccache";
      assert attrs.CARGO_INCREMENTAL == "0";
      assert wrapped.passthru.rsHarborBuildCacheWrapped;
        pkgs.runCommand "check-build-cache-policy-preserves-direct-env" {} "touch $out";

    # Fail if flake inputs ever point at retired forge mirrors again
    # (fleet migrated to github.com/caniko/*). sourceUrl package metadata
    # is informational only, never fetched, so it is excluded.
    site-host-pinning =
      pkgs.runCommand "check-site-host-pinning" {
        rootFlakeNix = ./flake.nix;
        rootFlakeLock = ./flake.lock;
        siteFlakeNix = ./site/flake.nix;
        siteFlakeLock = ./site/flake.lock;
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
  }
