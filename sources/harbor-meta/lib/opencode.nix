{lib}: let
  schema = "https://opencode.ai/config.json";
  formatPolicy = import ./format-policy.nix {inherit lib;};

  # `lsp = {}` (the default) renders a policy-only config: the format
  # permissions without any `lsp` block. A non-empty `lsp` is merged in
  # verbatim. `formatPermissions` toggles the deny fragment,
  # `extraFormatDenies` appends project-local patterns (the same twin
  # expansion applies to them), and `storePathTwins` gates the
  # `/nix/store/*/bin/…` and `./result/bin/…` variants. The
  # `formatPolicy.marker` identity key is written unconditionally so
  # harbor-opencode can always recognise its own output.
  mkConfig = {
    lsp ? {},
    openpencil ? false,
    formatPermissions ? true,
    extraFormatDenies ? [],
    storePathTwins ? true,
  }:
    {
      "$schema" = schema;
      "${formatPolicy.marker}" = formatPolicy.markerValue;
    }
    // lib.optionalAttrs (lsp != {}) {inherit lsp;}
    // lib.optionalAttrs openpencil {
      mcp.openpencil = {
        type = "local";
        command = ["openpencil-desktop" "--mcp" "{env:HOME}/.local/share/openpencil/agent.op"];
        enabled = true;
      };
    }
    // lib.optionalAttrs formatPermissions {
      permission.bash = formatPolicy.bashDenies {
        inherit storePathTwins;
        extra = extraFormatDenies;
      };
    };

  # Normalize one validated profile record. A profile is the only place a
  # language may introduce anything language-specific:
  #   lsp         — the `lsp` block merged into rendered configs
  #   detect      — `files` present in the project root and/or `flakeMarkers`
  #                 matching flake.nix make `--kind detect` select the profile
  #   packages    — pkgs → packages providing the LSP binaries (toolchain-
  #                 provided binaries such as rust-analyzer stay out)
  normalizeProfile = name: profile: let
    detect = profile.detect or {};
    detectList = field: value:
      assert lib.assertMsg (builtins.isList value && lib.all builtins.isString value)
      "harbor-meta.opencode: profile `${name}` detect.${field} must be a list of strings"; value;
    packages = profile.packages or (_pkgs: []);
  in {
    lsp =
      if profile ? lsp && builtins.isAttrs profile.lsp
      then profile.lsp
      else throw "harbor-meta.opencode: profile `${name}` needs an `lsp` attrset";
    detect = {
      files = detectList "files" (detect.files or []);
      flakeMarkers = detectList "flakeMarkers" (detect.flakeMarkers or []);
    };
    packages = assert lib.assertMsg (builtins.isFunction packages)
    "harbor-meta.opencode: profile `${name}` packages must be a function `pkgs: [...]\""; packages;
  };
in {
  inherit
    schema
    formatPolicy
    mkConfig
    ;

  configTextFor = args:
    builtins.toJSON (mkConfig args) + "\n";

  configPath = ".opencode/opencode.jsonc";

  # Validate an attribute set of language profiles. Names must be shell- and
  # case-pattern-safe (`[a-z][a-z0-9_-]*`) and must not collide with the
  # CLI's own `detect`/`none` keywords; unknown record keys fail closed so
  # a typo like `package` cannot silently drop behaviour.
  mkRegistry = profiles:
    assert lib.assertMsg (builtins.isAttrs profiles)
    "harbor-meta.opencode.mkRegistry: profiles must be an attrset";
      lib.mapAttrs (
        name: profile:
          assert lib.assertMsg (builtins.match "[a-z][a-z0-9_-]*" name != null)
          "harbor-meta.opencode: invalid profile name `${name}` (expected [a-z][a-z0-9_-]*)";
          assert lib.assertMsg (!(builtins.elem name ["detect" "none"]))
          "harbor-meta.opencode: profile name `${name}` is reserved by the CLI";
          assert lib.assertMsg
          (lib.all (key: builtins.elem key ["lsp" "detect" "packages"]) (builtins.attrNames profile))
          "harbor-meta.opencode: profile `${name}` has unknown keys (${lib.concatStringsSep ", " (builtins.attrNames profile)}); expected lsp, detect, packages";
            normalizeProfile name profile
      )
      profiles;

  # All subsets of a sorted profile-name list in deterministic order:
  # profileSubsets ["a" "b"] == [[] ["a"] ["b"] ["a" "b"]]. The CLI renders
  # one config document per subset (2^n), so registries stay small on
  # purpose.
  profileSubsets = names:
    assert lib.assertMsg (builtins.length names <= 6)
    "harbor-meta.opencode: at most 6 profiles per registry (2^n rendered configs)";
      builtins.foldl' (acc: name: acc ++ map (subset: subset ++ [name]) acc) [[]] names;

  # Canonical render key for a profile subset: registry-ordered names joined
  # by commas, `none` for the empty (policy-only) set. This key names the
  # `--kind` value the CLI resolves and the config it renders.
  profileKey = subset:
    if subset == []
    then "none"
    else lib.concatStringsSep "," subset;

  # Merged `lsp` block for a profile subset; later profiles win on key
  # clashes, so callers pass subsets in registry order.
  lspFor = registry: subset:
    lib.foldl' (acc: name: acc // registry.${name}.lsp) {} subset;
}
