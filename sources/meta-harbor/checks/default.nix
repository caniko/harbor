{
  pkgs,
  lib,
  harborOpencode,
}: {
  opencode-configs = pkgs.runCommand "meta-harbor-opencode-configs" {} ''
    cat > rust.json <<'EOF'
    ${lib.opencode.configText "rust"}
    EOF
    cat > python.json <<'EOF'
    ${lib.opencode.configText "python"}
    EOF
    grep -q rust-analyzer rust.json
    grep -q nixd rust.json
    grep -q taplo rust.json
    grep -q basedpyright-langserver python.json
    grep -q '"pyright":{"disabled":true}' python.json
    grep -q '"ruff"' python.json
    mkdir -p $out
    echo ok > $out/result
  '';

  harbor-opencode-sync-check = pkgs.runCommand "meta-harbor-opencode-sync-check" {} ''
    export PATH=${harborOpencode}/bin:$PATH
    mkdir -p project
    touch project/Cargo.toml
    harbor-opencode sync --kind detect --root project
    harbor-opencode check --kind detect --root project
    grep -q rust-analyzer project/.opencode/opencode.jsonc
    mkdir -p $out
    echo ok > $out/result
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
      pkgs.runCommand "meta-harbor-package-tests-plan-shape" {} ''
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
      pkgs.runCommand "meta-harbor-package-tests-validation" {} ''
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
      runnerBuilder = rendered.runnerBuilder;
    };
  in
    assert pkgs.lib.hasInfix "chocolatey/test-environment" vagrantfile;
    assert pkgs.lib.hasInfix ''config.vm.synced_folder "packages", "/packages"'' vagrantfile;
    assert pkgs.lib.hasInfix ''v.customize ["modifyvm", :id, "--memory", "6144"]'' vagrantfile;
    assert pkgs.lib.hasInfix ''v.customize ["modifyvm", :id, "--cpus", "4"]'' vagrantfile;
    assert pkgs.lib.hasInfix "choco install demo --version 1.0.0 --source C:\\packages" vagrantfile;
    assert rendered.runnerBuilder.kind == "chocolatey-vagrant-runner-builder";
    assert bundle.hierarchy == ["generic-builder" "windows-builder" "chocolatey-builder" "generic" "windows" "chocolatey-vagrant" "generic-runner-builder" "chocolatey-vagrant-runner-builder"];
      pkgs.runCommand "meta-harbor-package-tests-chocolatey-vagrant-render" {} ''
        test -f ${rendered.planJson}
        test -x ${rendered.runner}/bin/package-test-demo
        grep -q '"kind":"chocolatey-vagrant"' ${rendered.planJson}
        grep -q '"builderRef":"chocolatey-builder:demo:1.0.0"' ${rendered.planJson}
        grep -q 'vagrant up --provider=virtualbox' ${rendered.runner}/bin/package-test-demo
        mkdir -p $out
        echo ok > $out/result
      '';
}
