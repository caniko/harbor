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
  assert inputs.simit.rev == "2240b1d90c2151e5a66a04a743fbb14a313d4498";
  assert inputs.plinth-source.rev == "534dfa6dd6217db97b7ccd313501120a3c34d912";
    pkgs.runCommand "harbor-bootstrap-inputs" {} ''
      touch "$out"
    ''
