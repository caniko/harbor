{self}: {
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.programs.harbor;
  generic = lib.filter (name: !(lib.elem name ["cad" "blender" "llm"])) cfg.resolved.selected;
in {
  key = "harbor.homeManagerModules.default";
  imports = [
    (import ./options.nix {
      inherit self;
      kind = "home";
    })
    (import ./companions.nix {
      inherit self;
      kind = "home";
    })
    (import ./blender.nix {inherit self;})
    self.homeManagerModules.cad-default
    self.homeManagerModules.llm-default
    (lib.mkAliasOptionModule ["programs" "harbor" "tools" "cad" "package"] ["programs" "harbor-cad" "package"])
    (lib.mkAliasOptionModule ["programs" "harbor" "tools" "cad" "mcp" "package"] ["programs" "harbor-cad" "mcp" "package"])
    (lib.mkAliasOptionModule ["programs" "harbor" "tools" "llm" "package"] ["programs" "harborLlm" "package"])
  ];
  config = lib.mkIf cfg.enable {
    home.packages =
      map (name: cfg.tools.${name}.package) generic
      ++ lib.optional (cfg.tools.openpencil.desktop.enable && pkgs.stdenv.hostPlatform.system == "x86_64-linux") cfg.tools.openpencil.desktop.package;
    programs.harbor.tools.cad.mcp.socket = lib.mkDefault "${config.home.homeDirectory}/.local/state/harbor-cad/worker.sock";
    programs.harbor-cad = {
      enable = lib.mkDefault (lib.elem "cad" cfg.resolved.selected);
      mcp.enable = lib.mkDefault cfg.tools.cad.mcp.enable;
    };
    programs.harborLlm = {
      enable = lib.mkDefault (lib.elem "llm" cfg.resolved.selected);
    };
    # Harbor's nix-direnv: batched input roots, and builds that outlive an
    # interrupted activation. Only the package changes; direnv stays opt-in.
    programs.direnv.nix-direnv.package = lib.mkIf (self.packages.${pkgs.stdenv.hostPlatform.system} ? core-nix-direnv) (lib.mkDefault self.packages.${pkgs.stdenv.hostPlatform.system}.core-nix-direnv);
    xdg.configFile = lib.mapAttrs' (backend: manifest: lib.nameValuePair "harbor/cad/runtime-${backend}.json" {source = manifest;}) cfg.tools.cad.runtimeManifests;
  };
}
