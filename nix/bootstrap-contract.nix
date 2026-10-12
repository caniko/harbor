{
  self,
  inputs,
  pkgs,
}: let
  lock = builtins.fromJSON (builtins.readFile (self.outPath + "/flake.lock"));
  roots = lock.nodes.${lock.root}.inputs;
  sourceOnly = name: lock.nodes.${roots.${name}}.flake == false;
  currentTransport = node:
    builtins.all (entry:
      builtins.match ".*codeberg\\.org/caniko/.*" (entry.url or "") == null)
    [node.locked or {} node.original or {}];
in
  assert sourceOnly "simit";
  assert sourceOnly "plinth-source";
  assert builtins.all currentTransport (builtins.attrValues lock.nodes);
  assert inputs.simit.rev == "db0883dc27476235f37f5e6f0be6b214648c5302";
  assert inputs.plinth-source.rev == "4718cc09d9d1b54cc10e23e60c37b656bf7285a6";
    pkgs.runCommand "harbor-bootstrap-inputs" {} ''
      touch "$out"
    ''
