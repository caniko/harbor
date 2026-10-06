{
  pkgs,
  self,
}: let
  inherit (pkgs) lib;
  evaluate = settings:
    (lib.evalModules {
      modules = [
        (import ../nix/home.nix self)
        {
          options = {
            assertions = lib.mkOption {
              type = lib.types.listOf lib.types.attrs;
              default = [];
            };
            programs.opencode = {
              enable = lib.mkEnableOption "OpenCode";
              package = lib.mkOption {
                type = lib.types.package;
                default = pkgs.emptyDirectory;
              };
              settings = lib.mkOption {
                type = lib.types.attrs;
                default = {};
              };
            };
          };
        }
        settings
      ];
      specialArgs = {inherit pkgs;};
    }).config;
  disabled = evaluate {};
  v1 = evaluate {
    programs.opencode.enable = true;
    programs.opencode.package = pkgs.emptyDirectory // {harborLlmEnvironmentVersion = 1;};
    programs.harborLlm = {
      enable = true;
      opencode = {
        enable = true;
        apiVersion = "v1";
      };
    };
  };
  unsupported = evaluate {
    programs.opencode.enable = true;
    programs.harborLlm = {
      enable = true;
      opencode = {
        enable = true;
        apiVersion = "v1";
      };
    };
  };
  v2 = evaluate {
    programs.opencode.enable = true;
    programs.harborLlm = {
      enable = true;
      opencode = {
        enable = true;
        options = {
          roots = ["/workspaces/example"];
          serverURL = "http://127.0.0.1:4096";
        };
      };
    };
  };
in
  assert disabled.programs.opencode.settings == {} && disabled.assertions == [];
  assert lib.all (a: a.assertion) (v1.assertions ++ v2.assertions);
  assert !(lib.all (a: a.assertion) unsupported.assertions);
  assert v1.programs.opencode.settings ? plugin && !(v1.programs.opencode.settings ? plugins);
  assert v2.programs.opencode.settings ? plugins && !(v2.programs.opencode.settings ? plugin);
  assert (builtins.head v2.programs.opencode.settings.plugins).options.roots == ["/workspaces/example"];
    pkgs.writeText "harbor-llm-home-module" "ok"
