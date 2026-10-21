# nix-direnv with batched input roots and builds that survive an interrupted
# activation. Patch the installed script after resholve: its installation
# copies the original store input over unpacked-tree changes. Existing base
# patches and fixup hooks are retained.
{
  pkgs,
  nix-direnv ? pkgs.nix-direnv,
}:
nix-direnv.overrideAttrs (old: {
  postFixup =
    (old.postFixup or "")
    + "\n"
    + ''
      chmod u+w "$out/share/nix-direnv/direnvrc"
      ${pkgs.lib.getExe pkgs.patch} --directory="$out" --strip=1 < ${pkgs.replaceVars ./nix-direnv-gcroots.patch {inherit (pkgs) coreutils;}}
      ${pkgs.lib.getExe pkgs.patch} --directory="$out" --strip=1 < ${pkgs.replaceVars ./nix-direnv-detached-realize.patch {
        inherit (pkgs) coreutils;
        flock = pkgs.lib.getExe pkgs.flock;
      }}
      chmod u-w "$out/share/nix-direnv/direnvrc"
    '';
})
