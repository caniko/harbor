{packageTests}: {
  pkgs,
  chocoPackage,
  provider ? "virtualbox",
  boxVersion ? null,
  gui ? false,
  cpus ? 4,
  memoryMiB ? 6144,
  verifyPowerShell ? [],
  keepVm ? false,
}: let
  plan = packageTests.mkChocolateyVagrantPlan {
    packageName = chocoPackage.id;
    version = chocoPackage.version;
    nupkg = toString chocoPackage.nupkgPath;
    builder = chocoPackage.artifactBuilder;
    inherit provider boxVersion gui cpus memoryMiB verifyPowerShell keepVm;
  };
  rendered = packageTests.mkPackageTestRunner {inherit pkgs plan;};
  environmentDir = pkgs.runCommand "${chocoPackage.id}-chocolatey-test-environment" {} ''
    mkdir -p "$out/packages"
    cp ${chocoPackage.nupkgPath} "$out/packages/${chocoPackage.id}.${chocoPackage.version}.nupkg"
    cp ${rendered.planJson} "$out/plan.json"
    cp ${pkgs.writeText "Vagrantfile" (packageTests.renderVagrantfile plan)} "$out/Vagrantfile"
  '';
  runnerBuilder =
    rendered.runnerBuilder
    // {
      environment = toString environmentDir;
      metadata =
        rendered.runnerBuilder.metadata
        // {
          environmentDir = toString environmentDir;
        };
    };
in {
  inherit plan environmentDir runnerBuilder;
  artifactBuilder = chocoPackage.artifactBuilder;
  inherit (rendered) runner app;
}
