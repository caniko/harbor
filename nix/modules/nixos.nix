{self}: {
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.programs.harbor;
  generic = lib.filter (name: name != "cad") cfg.resolved.selected;
in {
  key = "harbor.nixosModules.default";
  imports = [
    (import ./options.nix {
      inherit self;
      kind = "nixos";
    })
    (import ./companions.nix {
      inherit self;
      kind = "nixos";
    })
    self.nixosModules.cad-default
    self.nixosModules.rust-buildCache
    self.nixosModules.rust-macosSdk
    self.nixosModules.database-default
    (lib.mkAliasOptionModule ["programs" "harbor" "tools" "cad" "package"] ["services" "harbor-cad" "package"])
  ];
  config = lib.mkIf cfg.enable {
    environment.systemPackages =
      map (name: cfg.tools.${name}.package) generic
      ++ lib.optional (cfg.tools.openpencil.desktop.enable && pkgs.stdenv.hostPlatform.system == "x86_64-linux" && builtins.elem "openpencil" cfg.resolved.selected) cfg.tools.openpencil.desktop.package
      ++ lib.optional (cfg.tools.cad.mcp.enable && builtins.elem "cad" cfg.resolved.selected) cfg.tools.cad.mcp.package
      ++ lib.optionals (cfg.tools.blender.mcp.enable && pkgs.stdenv.hostPlatform.isLinux && builtins.elem "blender" cfg.resolved.selected) [cfg.tools.blender.mcp.package cfg.tools.blender.mcp.extensionPackage];
    services.harbor-cad = {
      enable = lib.mkDefault (lib.elem "cad" cfg.resolved.selected);
    };
    environment.etc = lib.mapAttrs' (backend: manifest: lib.nameValuePair "harbor/cad/runtime-${backend}.json" {source = manifest;}) cfg.tools.cad.runtimeManifests;
  };
}
