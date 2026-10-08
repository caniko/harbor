# Package Tests

`harbor-rs` exposes package build and package-test plans through `harbor-meta`
so downstream flakes can describe package verification consistently before each
package format has a runnable backend.

The hierarchy is:

- artifact builders for package outputs
- generic package test plans for any package artifact
- Windows package test plans for PowerShell-based installers
- runner builders for local test environments
- Chocolatey Vagrant plans backed by the community Chocolatey test environment

Existing package helpers expose an `artifactBuilder` field on their result where
they have a concrete output: `mkAppImage`, `mkDebPackage`, `mkCoprSpec`,
`mkFlatpakManifest`, `mkHomebrewFormula`, `mkScoopManifest`, `mkChocoPackage`,
`mkAndroidApk`, `mkAndroidApkDevBuilder`, and `mkTrunkPackage`. Only Chocolatey
has a runnable VM helper today. Other package formats can still emit normalized
plans with `unsupportedRunnerReason` so release tooling can surface the missing
local backend explicitly.

## Chocolatey

```nix
let
  choco = harbor-rs.lib.mkChocoPackage {
    inherit pkgs;
    id = "my-app";
    version = "1.0.0";
    description = "Example Windows CLI";
    homepage = "https://example.com";
    license = "MIT";
    licenseUrl = "https://example.com/LICENSE";
    authors = ["Example"];
    architectures.x64 = {
      url = "https://example.com/my-app-1.0.0.zip";
      sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    };
  };

  chocoTest = harbor-rs.lib.mkChocoTestEnvironment {
    inherit pkgs;
    chocoPackage = choco;
    verifyPowerShell = [''
      my-app --version
    ''];
  };
in {
  packages.choco = choco.nupkgPath;
  apps.choco-test = chocoTest.app;
}
```

`mkChocoPackage` exposes `artifactBuilder`, with `output` pointing at the
generated `.nupkg`. `mkChocoTestEnvironment` carries that builder into its test
plan and returns a `runnerBuilder` for the generated Vagrant environment.

`apps.choco-test` writes a local `.package-test/<package>` directory, stages the
`.nupkg` under `packages/`, writes a generated `Vagrantfile`, and then runs the
Chocolatey install inside the Vagrant VM.

The generated Vagrantfile uses the `chocolatey/test-environment` box, syncs
`packages` to `C:\packages`, and defaults to verifier-like VirtualBox settings:
headless, 4 CPUs, 6144 MiB RAM, and disabled clipboard/drag-and-drop.

## Generic Plans

Use `mkPackageArtifactBuilder` and `mkPackageTestPlan` when a package format
does not yet have a runner:

```nix
let
  deb = harbor-rs.lib.mkDebPackage {
    inherit pkgs;
    packageName = "my-app";
    version = "1.0.0";
    arch = "amd64";
    maintainer = "Example <example@example.com>";
    description = "Example CLI";
    files = [
      {
        source = "${self.packages.${system}.my-app}/bin/my-app";
        target = "/usr/bin/my-app";
        mode = "0755";
      }
    ];
  };
in harbor-rs.lib.mkPackageTestPlan {
  inherit pkgs;
  kind = "debian";
  artifactBuilder = deb.artifactBuilder;
  installCommand = "apt install ./my-app.deb";
  unsupportedRunnerReason = "No generic Debian VM runner is implemented yet.";
}
```

Use `mkPackageArtifactBuilder` directly when a downstream packaging helper is not
implemented in `harbor-rs` yet but still has a concrete output path.
