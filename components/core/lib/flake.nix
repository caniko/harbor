{lib}: {
  mkDerivationManifest = {
    system,
    targets,
  }: let
    manifestFor = name: target: let
      context = builtins.getContext (builtins.toString target);
      derivations =
        lib.filterAttrs (
          path: value: lib.hasSuffix ".drv" path && value ? outputs
        )
        context;
      paths = builtins.attrNames derivations;
      validPath = lib.assertMsg (
        builtins.length paths == 1
      ) "mkDerivationManifest: target ${name} must reference exactly one derivation";
      drvPath = builtins.head paths;
      outputs = lib.sort builtins.lessThan derivations.${drvPath}.outputs;
      validOutputs = lib.assertMsg (
        outputs != []
      ) "mkDerivationManifest: target ${name} must select at least one output";
    in
      assert validPath;
      assert validOutputs; {
        drvPath = builtins.unsafeDiscardStringContext drvPath;
        inherit outputs;
      };
  in
    assert lib.assertMsg (system != "") "mkDerivationManifest: system must not be empty"; {
      schemaVersion = 1;
      inherit system;
      targets = lib.mapAttrs manifestFor targets;
    };
}
