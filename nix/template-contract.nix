{
  self,
  inputs,
  pkgs,
}: let
  inherit (pkgs) lib;
  templates = [
    "android/templates/default"
    "evm/templates/default"
    "go/templates/default"
    "javascript/templates/default"
    "javascript/templates/node"
    "projects/templates/default"
    "projects/templates/default/site"
    "python/templates/default"
    "rust/templates/default"
    "rust/templates/bevy"
    "solana/templates/default"
    "tex/templates/default"
  ];
  qualifiedRevision = "7d99eb50c52d0a941e2996b97c469b32a7657ef4";
  qualifiedUrl = "git+https://github.com/caniko/harbor.git?ref=feat/harbor-monorepo-components&rev=${qualifiedRevision}";
  evaluate = name: let
    source = self.outPath + "/components/${name}";
    flake = import (source + "/flake.nix");
    selectedInputs = lib.mapAttrs (input: _:
      if input == "harbor"
      then self
      else inputs.${input})
    flake.inputs;
    # The site template is independent of Harbor's reusable documentation
    # APIs. Reuse the bootstrap generator's pinned site input for evaluation.
    arguments =
      selectedInputs
      // lib.optionalAttrs (flake.inputs ? plinth) {
        plinth = inputs.simit.inputs.plinth;
      };
    templateSelf =
      outputs
      // {
        outPath = source;
        __toString = _: toString source;
      };
    outputs = flake.outputs (arguments // {self = templateSelf;});
    lockConsistent =
      if builtins.pathExists (source + "/flake.lock")
      then let
        lock = builtins.fromJSON (builtins.readFile (source + "/flake.lock"));
        roots = lock.nodes.${lock.root}.inputs;
      in
        roots ? harbor
        && builtins.isString roots.harbor
        && lock.nodes.${roots.harbor}.locked.rev == qualifiedRevision
        && builtins.all (input: !(lib.hasPrefix "harbor-" input)) (builtins.attrNames roots)
        && builtins.all (input: let
          declaration = flake.inputs.${input};
        in
          !(declaration ? follows) || roots.${input} == lib.splitString "/" declaration.follows) (builtins.attrNames flake.inputs)
      else true;
    system = "x86_64-linux";
    derivations = output: lib.mapAttrs (_: value: value.drvPath) (outputs.${output}.${system} or {});
  in
    builtins.addErrorContext "while evaluating Harbor template ${name}" (
      assert flake.inputs.harbor.url == qualifiedUrl;
      assert lib.assertMsg lockConsistent "Harbor template ${name} has a stale input lock";
      assert builtins.all (input: !(lib.hasPrefix "harbor-" input)) (builtins.attrNames flake.inputs);
      assert builtins.all (input: !(input ? follows) || lib.hasPrefix "harbor/" input.follows) (builtins.attrValues flake.inputs); {
        packages = derivations "packages";
        devShells = derivations "devShells";
        formatter = (outputs.formatter.${system} or null).drvPath or null;
      }
    );
  evaluated = lib.genAttrs templates evaluate;
  supportedPythonSystems = ["x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin"];
in
  assert self.lib.python.allSystems == supportedPythonSystems;
  assert builtins.attrNames (self.lib.python.forAllSystems (_: true)) == builtins.sort builtins.lessThan supportedPythonSystems;
    builtins.deepSeq evaluated (pkgs.runCommand "harbor-template-contract" {} ''
      mkdir -p "$out"
      cp ${pkgs.writeText "harbor-template-derivations.json" (builtins.unsafeDiscardStringContext (builtins.toJSON evaluated))} "$out/derivations.json"
    '')
