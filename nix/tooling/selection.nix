{lib}: let
  catalog = import ./catalog.nix;
  requested = frameworks: tool:
    lib.any (framework: lib.elem framework (["core"] ++ frameworks)) catalog.tools.${tool}.frameworks;
  supported = system: tool: lib.elem system catalog.tools.${tool}.systems;
in
  assert lib.assertMsg (lib.all (skill: lib.all (name: builtins.hasAttr name catalog.frameworks) skill.requires.frameworks) (builtins.attrValues catalog.skills)) "harbor: skill requires an unknown framework"; {
  inherit catalog requested supported;
  # Tools a skill's required frameworks realize on `system` (core excluded).
  skillTools = system: name:
    lib.filter (tool:
      supported system tool
      && lib.any (framework: lib.elem framework catalog.skills.${name}.requires.frameworks) (lib.remove "core" catalog.tools.${tool}.frameworks))
    (builtins.attrNames catalog.tools);
  defaultEnabled = {
    enable,
    frameworks,
    system,
    tool,
  }:
    enable && requested frameworks tool && supported system tool;
  resolve = {
    enable ? true,
    frameworks ? [],
    system,
    tools ? {},
  }:
    assert lib.assertMsg (lib.all (name: builtins.hasAttr name catalog.frameworks) frameworks) "harbor: unknown framework selection";
    assert lib.assertMsg (lib.all (name: builtins.hasAttr name catalog.tools) (builtins.attrNames tools)) "harbor: unknown tool selection"; let
      enabled = name: enable && (tools.${name}.enable or (requested frameworks name && supported system name));
      names = builtins.attrNames catalog.tools;
      unsupported = lib.filter (name:
        enable
        && !(supported system name)
        && (enabled name || (requested frameworks name && !catalog.tools.${name}.optional && !((tools.${name} or {}) ? enable && !tools.${name}.enable))))
      names;
    in {
      selected = lib.filter (name: enabled name && supported system name) names;
      inherit unsupported;
      unavailable = lib.filter (name: enable && requested frameworks name && !(supported system name)) names;
    };
}
