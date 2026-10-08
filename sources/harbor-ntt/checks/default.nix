{
  pkgs,
  self,
}: let
  mkNttArgs = builtins.functionArgs self.lib.mkNtt;
  mkShellArgs = builtins.functionArgs self.lib.mkNttDevShell;
in {
  api = assert mkNttArgs.pkgs == false;
  assert mkNttArgs.pins == false;
  assert mkNttArgs.anchorRustToolchain == false;
  assert mkShellArgs.extraPackages == true;
  assert mkShellArgs.extraEnv == true;
  assert mkShellArgs.extraShellHook == true;
    pkgs.runCommand "harbor-ntt-api" {} ''
      touch "$out"
    '';
}
