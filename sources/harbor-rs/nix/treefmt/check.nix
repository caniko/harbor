{
  pkgs,
  treefmt-nix,
  rustModule,
  baseModules,
  rustToolchain,
}: let
  standalone = (treefmt-nix.lib.evalModule pkgs {imports = [rustModule];}).config;
  composed =
    (treefmt-nix.lib.evalModule pkgs {
      imports = baseModules ++ [rustModule];
      projectRootFile = "flake.nix";
      programs.rustfmt.package = rustToolchain;
      programs.rustfmt.edition = "2024";
    }).config;
  unformattedNix = pkgs.writeText "unformatted.nix" "{ x=1; }\n";
  unformattedRs = pkgs.writeText "unformatted.rs" "fn main(){let x=1;println!(\"{}\",x);}\n";
in
  assert builtins.attrNames standalone.settings.formatter == ["rustfmt"];
  assert composed.programs.rustfmt.package == rustToolchain;
  assert composed.programs.rustfmt.edition == "2024";
  assert builtins.attrNames composed.settings.formatter == ["alejandra" "rustfmt" "taplo"];
    pkgs.runCommand "harbor-rs-treefmt-modules" {
      nativeBuildInputs = [composed.build.wrapper];
    } ''
      export HOME="$TMPDIR/home"
      mkdir -p "$HOME"
      cp ${unformattedNix} flake.nix
      cp ${unformattedRs} main.rs
      chmod u+w flake.nix main.rs
      treefmt --walk filesystem
      ! cmp -s ${unformattedNix} flake.nix
      ! cmp -s ${unformattedRs} main.rs
      treefmt --walk filesystem --clear-cache --fail-on-change
      touch "$out"
    ''
