{lib}: rec {
  eval = {
    flakeNix,
    inputs,
  }: let
    flake = import flakeNix;
    outputs = flake.outputs (
      inputs
      // {
        self =
          outputs
          // {
            outPath = dirOf flakeNix;
            inherit inputs;
          };
      }
    );
  in
    outputs;

  mkCheck = {
    pkgs,
    system,
    flakeNix,
    inputs,
    name ? "template-${baseNameOf (dirOf flakeNix)}",
    requiredFiles ? ["flake.nix"],
    requiredInputs ? [],
    commands ? [],
    env ? {},
    hookContains ? [],
    runHook ? false,
    devShellTests,
  }: let
    root = dirOf flakeNix;
    flakeText = builtins.readFile flakeNix;
    missingFiles =
      builtins.filter (path: !(builtins.pathExists (root + "/${path}"))) requiredFiles;
    missingInputs =
      builtins.filter (inputName: !(lib.hasInfix inputName flakeText)) requiredInputs;
    outputs = eval {inherit flakeNix inputs;};
    shell = outputs.devShells.${system}.default;
  in
    assert lib.assertMsg (missingFiles == [])
    "templateTests: missing files ${lib.concatStringsSep ", " missingFiles}";
    assert lib.assertMsg (missingInputs == [])
    "templateTests: flake.nix missing inputs ${lib.concatStringsSep ", " missingInputs}";
    assert lib.assertMsg (outputs ? devShells)
    "templateTests: template has no devShells";
      devShellTests.mkCheck {
        inherit pkgs shell commands env hookContains runHook;
        inherit name;
      };
}
