{lib}: let
  # Identity marker written into every Harbor-rendered OpenCode config.
  # `harbor-opencode sync` uses it to tell its own output from a hand-written
  # config and refuses to clobber the latter without --force. opencode's config
  # normalizer only copies known top-level keys into the effective config
  # (nativeAtomic whitelist in packages/core/src/config/normalize.ts), so the
  # unknown marker key is silently ignored at runtime — verified against the
  # deployed caniko/opencode@f18083c78e (v2.0.12) source.
  marker = "harbor.meta/opencode-config";
  markerValue = "1";

  # Every formatter invocation the agent policy denies. Each entry owns one
  # owner/tool row from docs/treefmt.md (or the fleet/root policy tools that
  # treefmt orchestrates) and lists its deny patterns:
  #
  #   - whole-tool formatters get `<tool> *` (the trailing ` *` also matches
  #     the bare command, so `<tool>` alone is denied too);
  #   - multi-purpose tools only get their formatting subcommand so build,
  #     lint, and plan invocations stay governed by the global policy.
  #
  # Patterns are language-independent: they cover the whole fleet regardless
  # of which profile subset (or none at all) a project's config renders.
  registry = [
    {
      patterns = ["alejandra *"];
      source = "harbor-meta treefmtModules.nix (Alejandra)";
    }
    {
      patterns = ["taplo *"];
      source = "harbor-meta treefmtModules.toml (Taplo)";
    }
    {
      patterns = [
        "rustfmt *"
        "cargo fmt *"
      ];
      source = "harbor-rs treefmtModules.rust (rustfmt)";
    }
    {
      patterns = [
        "prettier *"
        "npx prettier *"
        "pnpm exec prettier *"
        "pnpm dlx prettier *"
        "yarn dlx prettier *"
        "bunx prettier *"
      ];
      source = "harbor-js treefmtModules.javascript (Prettier, incl. exec twins)";
    }
    {
      patterns = [
        "ruff format *"
        "ruff check *--fix*"
      ];
      source = "harbor-py treefmtModules.python (Ruff format; `ruff check` without --fix stays allowed)";
    }
    {
      patterns = ["google-java-format *"];
      source = "harbor-android treefmtModules.java (google-java-format)";
    }
    {
      patterns = ["ktfmt *"];
      source = "harbor-android treefmtModules.kotlin (ktfmt)";
    }
    {
      patterns = ["forge fmt *"];
      source = "harbor-eth treefmtModules.solidity (Forge fmt)";
    }
    {
      patterns = ["latexindent *"];
      source = "harbor-tex treefmtModules.latex (latexindent)";
    }
    {
      patterns = [
        "deadnix *"
        "gofmt *"
        "just --fmt *"
        "statix *"
        "statix-fix *"
        "terraform fmt *"
      ];
      source = "fleet treefmt programs (canix-toolbelt flake-modules/formatters.nix; statix runs as the statix-fix wrapper binary)";
    }
    {
      patterns = [
        "shfmt *"
        "shfmt -d *"
      ];
      source = "root policy (agent_safety allows shfmt -d; treefmt --fail-on-change is the sanctioned path)";
    }
    {
      patterns = [
        "tofu fmt *"
        "go fmt *"
        "nix fmt *"
      ];
      source = "root policy (OpenTofu/Go formatting, nix fmt realises arbitrary outputs)";
    }
  ];

  patterns = lib.unique (lib.concatMap (entry: entry.patterns) registry);

  # Expand one deny pattern into every key opencode must see. The deployed
  # matcher (caniko/opencode@f18083c78e) has no dedicated assignment parser —
  # `packages/core/src/util/bash-permission.ts` does not exist there — so an
  # environment-prefixed command only matches a pattern that spells out the
  # `VAR=… ` prefix, and a store/result binary only matches a path pattern.
  # Each variant is therefore a plain wildcard spelled out verbatim:
  #
  #   foo *                 the command itself
  #   *=* foo *             environment-prefixed invocation
  #   /nix/store/*/bin/foo *   store wrapper binary (storePathTwins)
  #   ./result/bin/foo *       nix build result binary (storePathTwins)
  #   plus both path forms with an environment prefix
  expandPattern = storePathTwins: pattern: let
    parts = lib.splitString " " pattern;
    cmd = builtins.head parts;
    rest = lib.concatStringsSep " " (builtins.tail parts);
    suffix = lib.optionalString (rest != "") " ${rest}";
    envPrefixed = lib.hasPrefix "*=* " pattern;
    # Store/result twins locate the binary as the first token, which only
    # works for literal command names — never for wildcard-bearing extras.
    binaryShaped = !(lib.hasInfix "*" cmd) && !(lib.hasInfix "=" cmd);
    pathForms = lib.optionals (storePathTwins && binaryShaped) [
      "/nix/store/*/bin/${cmd}${suffix}"
      "./result/bin/${cmd}${suffix}"
    ];
  in
    [pattern]
    ++ lib.optionals (!envPrefixed) ["*=* ${pattern}"]
    ++ pathForms
    ++ map (form: "*=* ${form}") pathForms;

  # The `permission.bash` map merged into every rendered config. Every value
  # is "deny"; there is deliberately no `"*": "deny"` (or ask) catch-all —
  # opencode defaults unmatched commands to "ask" (fail-closed), and a
  # catch-all would also swallow the global allow rules for treefmt/git/read
  # tools that projects are expected to inherit.
  bashDenies = {
    storePathTwins ? true,
    extra ? [],
  }:
    lib.listToAttrs (
      map (pattern: {
        name = pattern;
        value = "deny";
      }) (lib.unique (lib.concatMap (expandPattern storePathTwins) (patterns ++ extra)))
    );

  # Mirrors Wildcard.match from packages/core/src/util/wildcard.ts in the
  # deployed caniko/opencode@f18083c78e (v2.0.12) matcher: backslashes
  # normalize to `/`, ERE metacharacters are escaped, `*` becomes `.*`,
  # `?` becomes `.`, a trailing ` .*` becomes `( .*)?`, and the result is
  # anchored. builtins.match is a full-string POSIX ERE, the same anchoring
  # opencode gets from `new RegExp("^" + escaped + "$")`. Parity holds only
  # for single-line inputs: Nix's `.` crosses newlines (measured:
  # `builtins.match "a.*b" "a\nb"` succeeds) while the deployed JS matcher
  # without the `s` flag does not. Checks must therefore replay only
  # single-line representative commands — multi-line shell-wrapper blobs
  # are resolved to their inner single-line invocation by an explicit
  # adapter, never matched as a blob. Keep in sync with the deployed
  # matcher when opencode is bumped.
  matcher = rec {
    globToEre = glob: let
      normalized = lib.replaceStrings ["\\"] ["/"] glob;
      escaped =
        lib.replaceStrings
        ["." "+" "^" "$" "{" "}" "(" ")" "|" "[" "]"]
        ["\\." "\\+" "\\^" "\\$" "\\{" "\\}" "\\(" "\\)" "\\|" "\\[" "\\]"]
        normalized;
      starred = lib.replaceStrings ["*"] [".*"] escaped;
      questioned = lib.replaceStrings ["?"] ["."] starred;
    in
      if lib.hasSuffix " .*" questioned
      then lib.removeSuffix " .*" questioned + "( .*)?"
      else questioned;

    match = pattern: input: builtins.match (globToEre pattern) input != null;
  };
in {
  inherit
    marker
    markerValue
    registry
    patterns
    expandPattern
    bashDenies
    matcher
    ;
}
