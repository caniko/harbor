{lib}: let
  tooling = import ../selection.nix {inherit lib;};
  select = args: tooling.resolve ({system = "x86_64-linux";} // args);
  disabled = select {
    enable = false;
    frameworks = ["cad"];
  };
  cad = select {frameworks = ["cad"];};
  withoutBlender = select {
    frameworks = ["cad"];
    tools.blender.enable = false;
  };
  darwin = select {
    system = "aarch64-darwin";
    frameworks = ["javascript"];
  };
  unsupported = select {
    system = "aarch64-darwin";
    frameworks = ["cad"];
  };
  explicitUnsupported = select {
    system = "aarch64-darwin";
    tools.llm.enable = true;
  };
in
  assert disabled.selected == [];
  assert lib.elem "cad" cad.selected && lib.elem "blender" cad.selected;
  assert lib.elem "openpencil" (select {frameworks = ["javascript"];}).selected;
  assert !(lib.elem "blender" withoutBlender.selected);
  assert darwin.unsupported == [] && lib.elem "node" darwin.selected;
  assert lib.elem "cad" unsupported.unsupported;
  assert explicitUnsupported.unsupported == ["llm"];
  assert !(builtins.tryEval (builtins.deepSeq (select {frameworks = ["typo"];}) true)).success;
  assert !(builtins.tryEval (builtins.deepSeq (select {tools.typo.enable = true;}) true)).success;
  assert (select {frameworks = ["rust" "rust"];}).selected == (select {frameworks = ["rust"];}).selected; true
