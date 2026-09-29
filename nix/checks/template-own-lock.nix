{
  pkgs,
  source,
  templates,
}: let
  checkTemplate = name: directory: let
    # The source hash locks the local template itself, allowing pure evaluation
    # with its complete lock graph, including follows and transitive overrides.
    template =
      builtins.getFlake (builtins.unsafeDiscardStringContext
        "path:${source.outPath}?narHash=${source.narHash}&dir=${directory}");
    root = source.outPath + "/${directory}";
    inherit (template.inputs) harbor-rs;
    templatePkgs = import template.inputs.nixpkgs {
      system = pkgs.stdenv.hostPlatform.system;
      overlays = [(import template.inputs.rust-overlay)];
    };
    toolchain = harbor-rs.lib.mkToolchain {
      pkgs = templatePkgs;
      toolchainProfile = "nightly";
    };
    treefmt = template.inputs.treefmt-nix.lib.evalModule templatePkgs (import (root + "/nix/treefmt.nix") {
      inherit harbor-rs;
      rustfmtPackage = toolchain.rustToolchain;
    });
    wrapper = template.formatter.${pkgs.stdenv.hostPlatform.system};
    hooks = import (root + "/nix/pre-commit.nix") {
      inherit harbor-rs;
      pkgs = templatePkgs;
      inherit (toolchain) rustToolchain;
      treefmtWrapper = wrapper;
    };
    context = "template-own-lock ${name}: pinned harbor-rs ${harbor-rs.rev}";
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
