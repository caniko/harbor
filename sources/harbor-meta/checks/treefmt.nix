{
  pkgs,
  treefmt-nix,
  modules,
}: let
  nixOnly = (treefmt-nix.lib.evalModule pkgs {imports = [modules.nix];}).config;
  tomlOnly = (treefmt-nix.lib.evalModule pkgs {imports = [modules.toml];}).config;
  composed =
    (treefmt-nix.lib.evalModule pkgs {
      imports = [modules.nix modules.toml];
      projectRootFile = "flake.nix";
      settings.excludes = ["generated/**"];
    }).config;
  reversed =
    (treefmt-nix.lib.evalModule pkgs {
      imports = [modules.toml modules.nix];
      projectRootFile = "flake.nix";
      settings.excludes = ["generated/**"];
    }).config;
  unformattedNix = pkgs.writeText "unformatted.nix" "{ x=1; }\n";
  unformattedToml = pkgs.writeText "unformatted.toml" "x=1\n";
in
  assert builtins.attrNames nixOnly.settings.formatter == ["alejandra"];
  assert builtins.attrNames tomlOnly.settings.formatter == ["taplo"];
  assert composed.settings == reversed.settings;
    pkgs.runCommand "harbor-meta-treefmt-modules" {
      nativeBuildInputs = [composed.build.wrapper];
    } ''
      export HOME="$TMPDIR/home"
      mkdir -p "$HOME" generated
      cp ${unformattedNix} flake.nix
      cp ${unformattedToml} sample.toml
      cp flake.nix generated/skip.nix
      chmod -R u+w .
      treefmt --walk filesystem
      ! cmp -s ${unformattedNix} flake.nix
      ! cmp -s ${unformattedToml} sample.toml
      cmp ${unformattedNix} generated/skip.nix
      treefmt --walk filesystem --clear-cache --fail-on-change
      touch "$out"
    ''
