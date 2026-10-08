# Regression suite for lib.treefmtScope (fleet composition contract).
#
# Eval-time asserts cover scoping, preservation, central overrides, identity
# reporting and every rejection case. The runCommand proofs below execute the
# real wrappers on fixture copies: per-scope options, exclusions, prettier
# config discovery from a foreign cwd, local-vs-global byte equivalence, and
# idempotence.
{
  pkgs,
  system,
  nixpkgs,
  lib,
  treefmt-nix,
  rust-overlay,
}: let
  scope = lib.treefmtScope;
  nlib = nixpkgs.lib;
  overlayPkgs = import nixpkgs {
    inherit system;
    overlays = [(import rust-overlay)];
  };
  # Fleet pin under test: simit-shaped modules inject nightly-only
  # skip_children, so the suite must run on nightly rustfmt.
  centralRustfmt =
    overlayPkgs.rust-bin.nightly."2026-09-15".default.override
    {extensions = ["rustfmt"];};
  centralRustfmtExe = "${centralRustfmt}/bin/rustfmt";

  fixtureA = {
    programs.alejandra = {
      enable = true;
      priority = 10;
    };
    programs.rustfmt = {
      enable = true;
      edition = "2021";
    };
    settings.formatter.rustfmt.options = ["--config" "max_width=40"];
    programs.taplo.enable = true;
  };

  # Deliberately different edition, options, defaults policy and prettier
  # coverage, plus a project-pinned taplo wrapper to exercise override
  # detection (still functional: it execs the same taplo).
  fixtureB = {
    enableDefaultExcludes = false;
    programs.alejandra.enable = true;
    programs.rustfmt = {
      enable = true;
      edition = "2024";
    };
    programs.taplo = {
      enable = true;
      package = pkgs.writeShellScriptBin "taplo" ''exec ${pkgs.taplo}/bin/taplo "$@"'';
    };
    programs.prettier = {
      enable = true;
      excludes = ["/generated/**"];
      includes = ["*.md"];
    };
  };

  # Root policy fixture: covers the workspace root while child subtrees are
  # composed alongside it. Root prettier uses printWidth 40 (children default
  # to 80) so the runtime proof can observe which rule formatted each file.
  fixtureRoot = {
    programs.alejandra.enable = true;
    programs.prettier = {
      enable = true;
      settings.printWidth = 40;
      includes = ["*.md"];
    };
  };

  projects = [
    {
      name = "proj-a";
      relPath = "fleet/proj-a";
      modules = [fixtureA];
    }
    {
      name = "proj-b";
      relPath = "fleet/proj-b";
      modules = [fixtureB];
    }
  ];

  allowedTaplo = ["fleet/proj-b:taplo"];

  composed = scope.compose {
    inherit treefmt-nix pkgs projects;
    centralPackages = {rustfmt = centralRustfmt;};
    allowedOverrides = allowedTaplo;
  };

  composedRoot = scope.compose {
    inherit treefmt-nix pkgs;
    centralPackages = {rustfmt = centralRustfmt;};
    allowedOverrides = allowedTaplo;
    projects =
      [
        {
          name = "root";
          relPath = "";
          isRoot = true;
          modules = [fixtureRoot];
        }
      ]
      ++ projects;
  };

  # The root fixture leaves default excludes on; reference them directly so
  # the per-formatter expectation below stays exact.
  rootDefaults =
    (treefmt-nix.lib.evalModule pkgs {
      imports = [fixtureRoot];
      projectRootFile = "flake.nix";
    }).config.settings.excludes;

  # Same composition plus an unselected-subtree glob: must land on root
  # formatter excludes only, never in the merged global list.
  extraExcluded = scope.compose {
    inherit treefmt-nix pkgs;
    centralPackages = {rustfmt = centralRustfmt;};
    allowedOverrides = allowedTaplo;
    rootExtraExcludes = ["fleet/unselected/**"];
    projects =
      [
        {
          name = "root";
          relPath = "";
          isRoot = true;
          modules = [fixtureRoot];
        }
      ]
      ++ projects;
  };

  formatters = composed.evalResult.config.settings.formatter;
  byName = name: formatters.${name};
  sourceOf = scopedName: let
    project =
      nlib.findFirst (project:
        (nlib.findFirst (formatter: formatter.name == scopedName) null
          project.formatters)
        != null)
      null
      composed.report.projects;
    match =
      if project == null
      then null
      else nlib.findFirst (formatter: formatter.name == scopedName) null project.formatters;
  in
    if match == null
    then null
    else match.source;

  mustFail = expr: !(builtins.tryEval (builtins.toJSON expr)).success;

  failReport = args: mustFail (scope.compose args).report;

  # Standalone local wrappers built from the same fixtures and toolchain.
  # The runtime proof formats identical copies locally and globally, then
  # requires byte-identical trees.
  localWrapper = fixture: edition:
    (treefmt-nix.lib.evalModule overlayPkgs {
      imports = [
        fixture
        {
          programs.rustfmt.package = nlib.mkForce centralRustfmt;
          programs.rustfmt.edition = nlib.mkForce edition;
        }
      ];
      projectRootFile = "flake.nix";
    }).config.build.wrapper;
  localA = localWrapper fixtureA "2021";
  localB = localWrapper fixtureB "2024";

  unformattedNix = pkgs.writeText "unformatted.nix" "{ x=1; }\n";
  unformattedRs = pkgs.writeText "unformatted.rs" "fn main() { let some_extremely_long_variable_name = 1; println!(\"{}\", some_extremely_long_variable_name); }\n";
  unformattedToml = pkgs.writeText "unformatted.toml" "x=1\n";
  unformattedMd = pkgs.writeText "unformatted.md" "# hi\n\nsome   text\n";
  prettierRc = pkgs.writeText "prettierrc" "{\"printWidth\": 80, \"proseWrap\": \"always\"}\n";
  excludedMd = pkgs.writeText "excluded.md" "# untouched\n";
  rootNix = pkgs.writeText "root.nix" "{ y=2; }\n";
  spacedMd = pkgs.writeText "spaced.md" "# spaced out\n\nmore    spaces\n";
  # Prose with a double space on a 64-char line: child printWidth 80
  # collapses the space but keeps one line; root printWidth 40 wraps.
  longMd = pkgs.writeText "long.md" "# Long\n\nThe  quick brown fox jumps over the lazy dog near the riverbank.\n";
  # Every treefmt invocation resolves its project root through this marker,
  # exactly like real checkouts. proj-a gets a formattable one inline.
  emptyFlake = pkgs.writeText "flake.nix" "{}\n";
in
  # --- scoping: bare names expand to direct-child AND nested forms, because
  # treefmt v2 requires at least one directory for `**/` (verified: `a/**/*.nix`
  # misses `a/f.nix` but hits `a/sub/f.nix`) ---
  assert (byName "fleet-proj-a-alejandra").includes == ["fleet/proj-a/*.nix" "fleet/proj-a/**/*.nix"];
  assert (byName "fleet-proj-a-alejandra").options == [];
  # --- preservation: priority and custom options survive composition ---
  assert (byName "fleet-proj-a-alejandra").priority == 10;
  assert builtins.sort builtins.lessThan (byName "fleet-proj-a-rustfmt").options
  == builtins.sort builtins.lessThan ["--config" "skip_children=true" "--edition" "2021" "--config" "max_width=40"];
  assert (byName "fleet-proj-b-rustfmt").options == ["--config" "skip_children=true" "--edition" "2024"];
  # --- central override: same pinned binary, distinct per-project editions ---
  assert (byName "fleet-proj-a-rustfmt").command == centralRustfmtExe;
  assert (byName "fleet-proj-b-rustfmt").command == centralRustfmtExe;
  assert sourceOf "fleet-proj-a-rustfmt" == "central";
  assert sourceOf "fleet-proj-b-rustfmt" == "central";
  # --- shared pkgs identity and project override detection ---
  assert sourceOf "fleet-proj-a-alejandra" == "shared-pkgs";
  assert sourceOf "fleet-proj-b-taplo" == "project-override";
  # --- prettier scoping: bare globs expand to both forms, rooted excludes stay rooted ---
  assert (byName "fleet-proj-b-prettier").includes == ["fleet/proj-b/*.md" "fleet/proj-b/**/*.md"];
  assert (byName "fleet-proj-b-prettier").excludes == ["fleet/proj-b/generated/**"];
  # --- disabled defaults survive: *.lock must not be excluded for proj-b ---
  assert !(nlib.elem "fleet/proj-b/*.lock"
    composed.evalResult.config.settings.excludes);
  # --- root marker forced once ---
  assert composed.evalResult.config.projectRootFile == "flake.nix";
  # --- root entry: every child subtree excluded from root formatters ---
  assert (composedRoot.evalResult.config.settings.formatter.root-alejandra).excludes
  == ["fleet/proj-a/**" "fleet/proj-b/**"] ++ rootDefaults;
  assert (composedRoot.evalResult.config.settings.formatter.root-alejandra).includes
  == ["*.nix"];
  assert (composedRoot.evalResult.config.settings.formatter.root-prettier).includes
  == ["*.md"];
  # --- root contributes no global excludes (child rules unaffected) ---
  # The root fixture leaves default excludes on; none of those bare patterns
  # may leak into the merged global list (children carry their own scoped
  # copies, e.g. `fleet/proj-a/**/*.lock`).
  assert builtins.all (pattern: !(nlib.elem pattern rootDefaults))
  (composedRoot.evalResult.config.settings.excludes or []);
  # --- rootExtraExcludes land on root formatters only, never globally ---
  assert nlib.elem "fleet/unselected/**"
  extraExcluded.evalResult.config.settings.formatter.root-alejandra.excludes;
  assert !(nlib.elem "fleet/unselected/**"
    (extraExcluded.evalResult.config.settings.excludes or []));
  # --- unresolved divergence fails wrapper preparation ---
  assert failReport {
    inherit treefmt-nix pkgs projects;
    centralPackages = {rustfmt = centralRustfmt;};
  };
  # --- rejections (must all fail) ---
  assert failReport {
    inherit treefmt-nix pkgs;
    centralPackages = {rustfmt = centralRustfmt;};
    allowedOverrides = allowedTaplo;
    projects = [
      {
        name = "overlap-parent";
        relPath = "fleet/proj-a";
        modules = [fixtureA];
      }
      {
        name = "overlap-child";
        relPath = "fleet/proj-a/nested";
        modules = [fixtureB];
      }
    ];
  };
  assert failReport {
    inherit treefmt-nix pkgs;
    centralPackages = {rustfmt = centralRustfmt;};
    allowedOverrides = allowedTaplo;
    projects = [
      {
        name = "slug-one";
        relPath = "fleet/a-b";
        modules = [fixtureA];
      }
      {
        name = "slug-two";
        relPath = "fleet/a/b";
        modules = [fixtureA];
      }
    ];
  };
  assert failReport {
    inherit treefmt-nix pkgs;
    centralPackages = {rustfmt = centralRustfmt;};
    allowedOverrides = allowedTaplo;
    projects = [
      {
        name = "final-collision";
        relPath = "fleet/a";
        modules = [
          {
            settings.formatter."b-c" = {
              command = "${pkgs.hello}/bin/hello";
              includes = ["*.txt"];
            };
          }
        ];
      }
      {
        name = "final-collision-other";
        relPath = "fleet/a-b";
        modules = [
          {
            settings.formatter."c" = {
              command = "${pkgs.hello}/bin/hello";
              includes = ["*.txt"];
            };
          }
        ];
      }
    ];
  };
  assert failReport {
    inherit treefmt-nix pkgs;
    centralPackages = {rustfmt = centralRustfmt;};
    allowedOverrides = allowedTaplo;
    projects = [
      {
        name = "dotdot";
        relPath = "fleet/../escape";
        modules = [fixtureA];
      }
    ];
  };
  assert failReport {
    inherit treefmt-nix pkgs;
    centralPackages = {rustfmt = centralRustfmt;};
    allowedOverrides = allowedTaplo;
    projects = [
      {
        name = "empty";
        relPath = "fleet/empty";
        modules = [];
      }
    ];
  };
  assert failReport {
    inherit treefmt-nix pkgs;
    centralPackages = {rustfmt = centralRustfmt;};
    allowedOverrides = allowedTaplo;
    projects = [
      {
        name = "relref";
        relPath = "fleet/relref";
        modules = [
          {
            settings.formatter.taplo = {
              command = "${pkgs.taplo}/bin/taplo";
              includes = ["*.toml"];
              options = ["--config" "./taplo.toml"];
            };
          }
        ];
      }
    ];
  };
    pkgs.runCommand "harbor-meta-treefmt-scope" {
      nativeBuildInputs = [
        composed.evalResult.config.build.wrapper
        localA
        localB
      ];
    } ''
      rootTreefmt=${composedRoot.evalResult.config.build.wrapper}/bin/treefmt
      export HOME="$TMPDIR/home"
      seed_tree() {
        root="$1"
        mkdir -p "$root/fleet/proj-a" "$root/fleet/proj-a/nested" "$root/fleet/proj-b/generated"
        cp ${unformattedNix} "$root/fleet/proj-a/flake.nix"
        cp ${unformattedNix} "$root/fleet/proj-a/nested/deep.nix"
        cp ${unformattedRs} "$root/fleet/proj-a/main.rs"
        cp ${emptyFlake} "$root/fleet/proj-b/flake.nix"
        cp ${unformattedToml} "$root/fleet/proj-b/sample.toml"
        cp ${unformattedRs} "$root/fleet/proj-b/main.rs"
        cp ${unformattedMd} "$root/fleet/proj-b/doc.md"
        cp ${unformattedMd} "$root/fleet/proj-b/my doc.md"
        cp ${longMd} "$root/fleet/proj-b/long.md"
        cp ${prettierRc} "$root/fleet/proj-b/.prettierrc"
        cp ${excludedMd} "$root/fleet/proj-b/generated/skip.md"
        cp ${rootNix} "$root/root-skip.nix"
        cp ${emptyFlake} "$root/flake.nix"
        chmod -R u+w "$root"
      }
      seed_tree case-local
      seed_tree case-global
      # Local wrappers run inside their own project roots...
      (cd case-local/fleet/proj-a && "${localA}/bin/treefmt" --walk filesystem .)
      (cd case-local/fleet/proj-b && "${localB}/bin/treefmt" --walk filesystem .)
      # ...while the global wrapper runs once from the parent over both subtrees.
      (cd case-global && treefmt --walk filesystem fleet/proj-a fleet/proj-b)
      # Byte equivalence between local and global formatting.
      diff -r case-local/fleet/proj-a case-global/fleet/proj-a
      diff -r case-local/fleet/proj-b case-global/fleet/proj-b
      # Expected changes happened, at both nesting depths (locks in both
      # arms of the bare-name expansion)...
      ! cmp -s ${unformattedNix} case-global/fleet/proj-a/flake.nix
      ! cmp -s ${unformattedNix} case-global/fleet/proj-a/nested/deep.nix
      ! cmp -s ${unformattedToml} case-global/fleet/proj-b/sample.toml
      ! cmp -s ${unformattedMd} case-global/fleet/proj-b/doc.md
      ! cmp -s ${unformattedMd} "case-global/fleet/proj-b/my doc.md"
      ! cmp -s ${longMd} case-global/fleet/proj-b/long.md
      # ...exclusions and non-selected files stayed byte-identical...
      cmp ${excludedMd} case-global/fleet/proj-b/generated/skip.md
      cmp ${rootNix} case-global/root-skip.nix
      # ...and a fresh-cache second pass is clean.
      (cd case-global && treefmt --walk filesystem --clear-cache --fail-on-change fleet/proj-a fleet/proj-b)
      # Root composition: seed a tree with root files plus one child copy...
      mkdir -p case-root/fleet/proj-b
      cp ${emptyFlake} case-root/flake.nix
      cp ${longMd} case-root/top.md
      cp ${rootNix} case-root/top.nix
      cp ${emptyFlake} case-root/fleet/proj-b/flake.nix
      cp ${longMd} case-root/fleet/proj-b/long.md
      cp ${prettierRc} case-root/fleet/proj-b/.prettierrc
      chmod -R u+w case-root
      (cd case-root && "$rootTreefmt" --walk filesystem .)
      # ...root files formatted per root policy (40-col wrap)...
      ! cmp -s ${longMd} case-root/top.md
      ! cmp -s ${rootNix} case-root/top.nix
      # ...while the child copy matches the child-rule output exactly,
      # proving root rules did not touch it despite the whole-tree walk.
      cmp case-global/fleet/proj-b/long.md case-root/fleet/proj-b/long.md
      (cd case-root && "$rootTreefmt" --walk filesystem --clear-cache --fail-on-change .)
      touch "$out"
    ''
