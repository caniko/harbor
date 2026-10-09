{
  pkgs,
  flakeCompat,
  source,
  templates,
}: let
  checkTemplate = name: directory: let
    root = source.outPath + "/${directory}";
    # Evaluate the complete lock graph, including follows and overrides, using
    # the pinned evaluator. Nix 2.35.1 aborts on getFlake's nested dir= paths.
    template =
      (import flakeCompat {
        src = root;
        system = pkgs.stdenv.hostPlatform.system;
      }).outputs;
    inherit (template.inputs) harbor;
    templatePkgs = import template.inputs.nixpkgs {
      system = pkgs.stdenv.hostPlatform.system;
      overlays = [(import template.inputs.rust-overlay)];
    };
    toolchain = harbor.lib.rust.mkToolchain {
      pkgs = templatePkgs;
      toolchainProfile = "nightly";
    };
    treefmt = template.inputs.treefmt-nix.lib.evalModule templatePkgs (import (root + "/nix/treefmt.nix") {
      inherit harbor;
      rustfmtPackage = toolchain.rustToolchain;
    });
    wrapper = template.formatter.${pkgs.stdenv.hostPlatform.system};
    hooks = import (root + "/nix/pre-commit.nix") {
      inherit harbor;
      pkgs = templatePkgs;
      inherit (toolchain) rustToolchain;
      treefmtWrapper = wrapper;
    };
    context = "template-own-lock ${name}: pinned Harbor ${harbor.rev}";
  in
    assert pkgs.lib.assertMsg (!builtins.hasAttr "cargo-fmt" hooks)
    "${context} still enables cargo-fmt alongside treefmt";
    assert pkgs.lib.assertMsg
    (builtins.attrNames hooks == ["cargo-audit" "cargo-clippy" "nix-flake-check" "treefmt"])
    "${context} must retain treefmt, clippy, audit, and the manual flake check";
    assert pkgs.lib.assertMsg
    (hooks.treefmt.enable && hooks.cargo-clippy.enable && hooks.cargo-audit.enable)
    "${context} has disabled required hooks";
    assert pkgs.lib.assertMsg
    (hooks.treefmt.package == wrapper && wrapper == treefmt.config.build.wrapper && pkgs.lib.hasPrefix "${wrapper}/bin/treefmt " hooks.treefmt.entry)
    "${context} does not use the template's configured treefmt wrapper";
    assert pkgs.lib.assertMsg
    (treefmt.config.programs.rustfmt.enable && treefmt.config.programs.rustfmt.package == toolchain.rustToolchain)
    "${context} does not format Rust with the pinned toolchain"; true;
in
  assert builtins.all (result: result) (pkgs.lib.mapAttrsToList checkTemplate templates);
    pkgs.runCommand "check-template-own-lock" {} "touch $out"
