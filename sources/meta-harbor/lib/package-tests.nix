{lib}: let
  supportedKinds = [
    "generic"
    "windows"
    "chocolatey-vagrant"
    "appimage"
    "flatpak"
    "copr-rpm"
    "debian"
    "homebrew"
    "scoop"
    "chocolatey"
  ];

  supportedArtifactBuilderKinds = [
    "generic-builder"
    "appimage-builder"
    "flatpak-builder"
    "copr-rpm-builder"
    "debian-builder"
    "homebrew-builder"
    "scoop-builder"
    "chocolatey-builder"
    "android-apk-builder"
    "android-apk-dev-builder"
    "trunk-builder"
  ];

  windowsArtifactBuilderKinds = [
    "scoop-builder"
    "chocolatey-builder"
  ];

  supportedRunnerBuilderKinds = [
    "generic-runner-builder"
    "chocolatey-vagrant-runner-builder"
  ];

  windowsKinds = [
    "windows"
    "chocolatey-vagrant"
    "scoop"
    "chocolatey"
  ];

  validPackageName = name:
    builtins.isString name && builtins.match "[A-Za-z0-9][A-Za-z0-9_.+-]*" name != null;

  validChocolateyName = name:
    builtins.isString name && builtins.match "[a-z][a-z0-9-]*" name != null;

  validVersion = version:
    builtins.isString version && version != "";

  validArtifacts = artifacts:
    builtins.isList artifacts
    && artifacts != []
    && lib.all (artifact:
      builtins.isAttrs artifact
      && artifact ? name
      && artifact ? path
      && builtins.isString artifact.name
      && artifact.name != ""
      && builtins.isString artifact.path
      && artifact.path != "")
    artifacts;

  validInstall = install:
    builtins.isAttrs install && install ? command && builtins.isString install.command && install.command != "";

  validVerify = verify:
    builtins.isList verify
    && lib.all (step:
      builtins.isAttrs step && step ? command && builtins.isString step.command && step.command != "")
    verify;

  validWindowsPath = path:
    builtins.isString path && builtins.match "[A-Za-z]:\\\\.*" path != null;

  validNullableCommand = command:
    command == null || (builtins.isString command && command != "");

  validInputs = inputs:
    builtins.isList inputs;

  artifactBuilderRef = builder: "${builder.kind}:${builder.packageName}:${builder.version}";

  artifactBuilderHierarchy = kind:
    if kind == "chocolatey-builder"
    then ["generic-builder" "windows-builder" "chocolatey-builder"]
    else if kind == "scoop-builder"
    then ["generic-builder" "windows-builder" "scoop-builder"]
    else ["generic-builder" kind];

  toPlanJson = plan: builtins.toJSON plan;

  escapeRuby = value: builtins.toJSON value;

  powershellLines = lines:
    lib.concatStringsSep "\n" (map (line: "    ${line}") lines);

  renderVagrantfile = plan: let
    runtime = plan.runtime;
    boxVersionLine =
      if runtime.boxVersion == null
      then ""
      else "  config.vm.box_version = ${escapeRuby runtime.boxVersion}\n";
    provider = runtime.provider;
    providerLine =
      if provider == "hyperv"
      then ''
        config.vm.provider :hyperv do |v, _override|
          v.memory = ${toString runtime.memoryMiB}
          v.maxmemory = nil
          v.cpus = ${toString runtime.cpus}
          v.ip_address_timeout = 240
          if Vagrant::VERSION >= '2.1.2'
            v.linked_clone = true
          else
            v.differencing_disk = true
          end
        end
      ''
      else ''
        config.vm.provider :virtualbox do |v, _override|
          v.gui = ${
          if runtime.gui
          then "true"
          else "false"
        }
          v.customize ["modifyvm", :id, "--memory", "${toString runtime.memoryMiB}"]
          v.customize ["modifyvm", :id, "--cpus", "${toString runtime.cpus}"]
          v.customize ["modifyvm", :id, "--clipboard", "disabled"]
          v.customize ["modifyvm", :id, "--draganddrop", "disabled"]
          v.customize ["modifyvm", :id, "--audio", "none"]
          v.linked_clone = true if Vagrant::VERSION >= '1.8.0'
        end
      '';
    provisionLines =
      [
        "$ErrorActionPreference = 'Stop'"
        "$ProgressPreference = 'SilentlyContinue'"
        "$installArgs = ${builtins.toJSON (lib.concatStringsSep " " plan.metadata.installArgs)}"
        "choco install ${plan.packageName} --version ${plan.version} --source ${plan.metadata.source} $installArgs"
      ]
      ++ (map (step: step.command) plan.verify);
  in ''
    Vagrant.configure("2") do |config|
      config.vm.box = ${escapeRuby runtime.box}
    ${boxVersionLine}  config.vm.guest = :windows
      config.vm.communicator = "winrm"
      config.winrm.username = "vagrant"
      config.winrm.password = "vagrant"
      config.winrm.port = 55985
      config.vm.boot_timeout = 1800
      config.winrm.max_tries = 900
      config.winrm.retry_delay = 2
      config.vm.synced_folder "packages", "/packages"
      config.vm.network :forwarded_port, guest: 5985, host: 55985, id: "winrm", auto_correct: true
      config.vm.network :forwarded_port, guest: 3389, host: 3389, id: "rdp", auto_correct: true
    ${providerLine}
      config.vm.provision :shell, inline: <<-'POWERSHELL', powershell_elevated_interactive: true
    ${powershellLines provisionLines}
      POWERSHELL
    end
  '';

  mkRunnerScript = {
    pkgs,
    plan,
    planJsonPath,
    vagrantfilePath,
  }: let
    artifact = builtins.head plan.artifacts;
  in
    pkgs.writeShellApplication {
      name = "package-test-${plan.packageName}";
      runtimeInputs = [
        pkgs.coreutils
        pkgs.findutils
        pkgs.gnugrep
      ];
      text = ''
        set -euo pipefail

        usage() {
          cat <<'USAGE'
        Usage: package-test-${plan.packageName} prepare|test|restore|destroy|status [WORKDIR]
        USAGE
        }

        cmd="''${1:-test}"
        case "$cmd" in
          -h|--help)
            usage
            exit 0
            ;;
        esac
        shift || true
        workdir="''${1:-$PWD/.package-test/${plan.packageName}}"

        prepare() {
          mkdir -p "$workdir/packages"
          cp ${lib.escapeShellArg artifact.path} "$workdir/packages/${artifact.name}"
          cp ${planJsonPath} "$workdir/plan.json"
          cp ${vagrantfilePath} "$workdir/Vagrantfile"
          printf 'prepared %s\n' "$workdir"
        }

        require_vagrant() {
          command -v vagrant >/dev/null 2>&1 || {
            printf 'package-test-${plan.packageName}: vagrant is required for Chocolatey VM tests\n' >&2
            exit 127
          }
        }

        case "$cmd" in
          prepare)
            prepare
            ;;
          test)
            prepare
            require_vagrant
            cd "$workdir"
            vagrant up --provider=${plan.runtime.provider}
            if vagrant snapshot list 2>/dev/null | grep -qx 'good'; then
              vagrant snapshot restore good --no-provision
            else
              vagrant snapshot save good
            fi
            vagrant provision
            ${lib.optionalString (!(plan.runtime.keepVm or false)) "vagrant halt"}
            ;;
          restore)
            require_vagrant
            cd "$workdir"
            vagrant snapshot restore good --no-provision
            ;;
          destroy)
            require_vagrant
            cd "$workdir"
            vagrant destroy -f
            ;;
          status)
            require_vagrant
            cd "$workdir"
            vagrant status
            ;;
          *)
            usage >&2
            exit 2
            ;;
        esac
      '';
    };
in rec {
  inherit supportedKinds windowsKinds toPlanJson renderVagrantfile;

  mkArtifactBuilder = {
    kind,
    packageName,
    version,
    output,
    buildCommand ? null,
    inputs ? [],
    metadata ? {},
    unsupportedBuilderReason ? null,
  }:
    assert lib.assertMsg (builtins.elem kind supportedArtifactBuilderKinds)
    "meta-harbor.packageTests.mkArtifactBuilder: unsupported kind `${kind}`";
    assert lib.assertMsg (validPackageName packageName)
    "meta-harbor.packageTests.mkArtifactBuilder: packageName must be non-empty and package-like";
    assert lib.assertMsg (validVersion version)
    "meta-harbor.packageTests.mkArtifactBuilder: version must be a non-empty string";
    assert lib.assertMsg (builtins.isString output && output != "")
    "meta-harbor.packageTests.mkArtifactBuilder: output must be a non-empty string";
    assert lib.assertMsg (validNullableCommand buildCommand)
    "meta-harbor.packageTests.mkArtifactBuilder: buildCommand must be null or a non-empty string";
    assert lib.assertMsg (validInputs inputs)
    "meta-harbor.packageTests.mkArtifactBuilder: inputs must be a list"; {
      inherit kind packageName version output buildCommand inputs metadata unsupportedBuilderReason;
      ref = "${kind}:${packageName}:${version}";
      hierarchy = artifactBuilderHierarchy kind;
    };

  mkRunnerBuilder = {
    kind,
    packageName,
    plan,
    runner,
    environment ? null,
    prepareCommand ? null,
    testCommand ? null,
    metadata ? {},
  }:
    assert lib.assertMsg (builtins.elem kind supportedRunnerBuilderKinds)
    "meta-harbor.packageTests.mkRunnerBuilder: unsupported kind `${kind}`";
    assert lib.assertMsg (validPackageName packageName)
    "meta-harbor.packageTests.mkRunnerBuilder: packageName must be non-empty and package-like";
    assert lib.assertMsg (builtins.isAttrs plan && plan ? kind && plan ? packageName)
    "meta-harbor.packageTests.mkRunnerBuilder: plan must be a package test plan";
    assert lib.assertMsg (builtins.isString runner && runner != "")
    "meta-harbor.packageTests.mkRunnerBuilder: runner must be a non-empty string";
    assert lib.assertMsg (environment == null || (builtins.isString environment && environment != ""))
    "meta-harbor.packageTests.mkRunnerBuilder: environment must be null or a non-empty string";
    assert lib.assertMsg (validNullableCommand prepareCommand)
    "meta-harbor.packageTests.mkRunnerBuilder: prepareCommand must be null or a non-empty string";
    assert lib.assertMsg (validNullableCommand testCommand)
    "meta-harbor.packageTests.mkRunnerBuilder: testCommand must be null or a non-empty string"; {
      inherit kind packageName plan runner environment prepareCommand testCommand metadata;
      ref = "${kind}:${packageName}:${plan.kind}";
      hierarchy =
        if kind == "chocolatey-vagrant-runner-builder"
        then ["generic-runner-builder" "chocolatey-vagrant-runner-builder"]
        else ["generic-runner-builder"];
    };

  mkPlan = {
    kind,
    packageName,
    version,
    artifacts,
    install,
    verify ? [],
    runtime ? {},
    metadata ? {},
    builder ? null,
  }:
    assert lib.assertMsg (builtins.elem kind supportedKinds)
    "meta-harbor.packageTests.mkPlan: unsupported kind `${kind}`";
    assert lib.assertMsg (validPackageName packageName)
    "meta-harbor.packageTests.mkPlan: packageName must be non-empty and package-like";
    assert lib.assertMsg (validVersion version)
    "meta-harbor.packageTests.mkPlan: version must be a non-empty string";
    assert lib.assertMsg (validArtifacts artifacts)
    "meta-harbor.packageTests.mkPlan: artifacts must be a non-empty list of { name, path }";
    assert lib.assertMsg (validInstall install)
    "meta-harbor.packageTests.mkPlan: install must include a non-empty command";
    assert lib.assertMsg (validVerify verify)
    "meta-harbor.packageTests.mkPlan: verify entries must include non-empty commands";
    assert lib.assertMsg (builder == null || (builtins.isAttrs builder && builder ? kind && builder ? output))
    "meta-harbor.packageTests.mkPlan: builder must be null or an artifact builder"; let
      testHierarchy =
        if kind == "chocolatey-vagrant"
        then ["generic" "windows" "chocolatey-vagrant"]
        else if builtins.elem kind windowsKinds
        then ["generic" "windows" kind]
        else ["generic" kind];
    in {
      inherit kind packageName version artifacts install verify runtime metadata builder;
      builderRef =
        if builder == null
        then null
        else artifactBuilderRef builder;
      hierarchy =
        (lib.optionals (builder != null) builder.hierarchy)
        ++ testHierarchy;
    };

  mkWindowsPlan = {
    packageName,
    version,
    artifacts,
    installPowerShell,
    verifyPowerShell ? [],
    runtime ? {},
    metadata ? {},
    builder ? null,
  }:
    assert lib.assertMsg (builtins.isString installPowerShell && installPowerShell != "")
    "meta-harbor.packageTests.mkWindowsPlan: installPowerShell must be non-empty";
    assert lib.assertMsg (builtins.isList verifyPowerShell && lib.all (step: builtins.isString step && step != "") verifyPowerShell)
    "meta-harbor.packageTests.mkWindowsPlan: verifyPowerShell must be a list of non-empty strings";
      mkPlan {
        kind = "windows";
        inherit packageName version artifacts runtime metadata builder;
        install = {
          shell = "powershell";
          command = installPowerShell;
        };
        verify =
          map (command: {
            shell = "powershell";
            inherit command;
          })
          verifyPowerShell;
      };

  mkChocolateyVagrantPlan = {
    packageName,
    version,
    nupkg,
    source ? "C:\\packages",
    installArgs ? ["-fdvy"],
    provider ? "virtualbox",
    box ? "chocolatey/test-environment",
    boxVersion ? null,
    gui ? false,
    cpus ? 4,
    memoryMiB ? 6144,
    commandTimeoutSeconds ? 720,
    verifyPowerShell ? [],
    keepVm ? false,
    builder ? null,
  }:
    assert lib.assertMsg (validChocolateyName packageName)
    "meta-harbor.packageTests.mkChocolateyVagrantPlan: packageName must match Chocolatey id rules";
    assert lib.assertMsg (builtins.isString nupkg && nupkg != "")
    "meta-harbor.packageTests.mkChocolateyVagrantPlan: nupkg must be a non-empty path string";
    assert lib.assertMsg (validWindowsPath source)
    "meta-harbor.packageTests.mkChocolateyVagrantPlan: source must be an absolute Windows path like C:\\packages";
    assert lib.assertMsg (builtins.elem provider ["virtualbox" "hyperv"])
    "meta-harbor.packageTests.mkChocolateyVagrantPlan: provider must be virtualbox or hyperv";
    assert lib.assertMsg (builtins.isList installArgs && lib.all builtins.isString installArgs)
    "meta-harbor.packageTests.mkChocolateyVagrantPlan: installArgs must be a list of strings";
    assert lib.assertMsg (builtins.isList verifyPowerShell && lib.all (step: builtins.isString step && step != "") verifyPowerShell)
    "meta-harbor.packageTests.mkChocolateyVagrantPlan: verifyPowerShell must be a list of non-empty strings";
      mkPlan {
        kind = "chocolatey-vagrant";
        inherit packageName version builder;
        artifacts = [
          {
            name = "${packageName}.${version}.nupkg";
            path = nupkg;
          }
        ];
        install = {
          shell = "powershell";
          command = "choco install ${packageName} --version ${version} --source ${source} ${lib.concatStringsSep " " installArgs}";
        };
        verify =
          map (command: {
            shell = "powershell";
            inherit command;
          })
          verifyPowerShell;
        runtime = {
          inherit provider box boxVersion gui cpus memoryMiB commandTimeoutSeconds keepVm;
        };
        metadata = {
          inherit source installArgs;
          upstream = "https://github.com/chocolatey-community/chocolatey-test-environment";
        };
      };

  mkPackageTestRunner = {
    pkgs,
    plan,
  }: let
    planJson = pkgs.writeText "${plan.packageName}-package-test-plan.json" (toPlanJson plan);
    vagrantfile =
      if plan.kind == "chocolatey-vagrant"
      then pkgs.writeText "Vagrantfile" (renderVagrantfile plan)
      else null;
    runner =
      if plan.kind == "chocolatey-vagrant"
      then
        mkRunnerScript {
          inherit pkgs plan;
          planJsonPath = planJson;
          vagrantfilePath = vagrantfile;
        }
      else throw "meta-harbor.packageTests.mkPackageTestRunner: no runnable backend for kind `${plan.kind}`";
    runnerBuilder = mkRunnerBuilder {
      kind = "chocolatey-vagrant-runner-builder";
      packageName = plan.packageName;
      inherit plan;
      runner = "${runner}/bin/package-test-${plan.packageName}";
      prepareCommand = "package-test-${plan.packageName} prepare";
      testCommand = "package-test-${plan.packageName} test";
      metadata = {
        provider = plan.runtime.provider;
        box = plan.runtime.box;
      };
    };
  in {
    inherit planJson runner runnerBuilder;
    app = {
      type = "app";
      program = "${runner}/bin/package-test-${plan.packageName}";
    };
  };

  mkBuildTestBundle = {
    artifactBuilder,
    plan,
    runnerBuilder ? null,
  }:
    assert lib.assertMsg (builtins.isAttrs artifactBuilder && artifactBuilder ? kind && artifactBuilder ? output)
    "meta-harbor.packageTests.mkBuildTestBundle: artifactBuilder must be an artifact builder";
    assert lib.assertMsg (builtins.isAttrs plan && plan ? kind && plan ? artifacts)
    "meta-harbor.packageTests.mkBuildTestBundle: plan must be a package test plan";
    assert lib.assertMsg (runnerBuilder == null || (builtins.isAttrs runnerBuilder && runnerBuilder ? kind && runnerBuilder ? runner))
    "meta-harbor.packageTests.mkBuildTestBundle: runnerBuilder must be null or a runner builder"; {
      inherit artifactBuilder plan runnerBuilder;
      hierarchy =
        (
          if plan.builderRef == artifactBuilder.ref
          then plan.hierarchy
          else artifactBuilder.hierarchy ++ plan.hierarchy
        )
        ++ lib.optionals (runnerBuilder != null) runnerBuilder.hierarchy;
    };
}
