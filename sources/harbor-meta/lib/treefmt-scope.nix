# Fleet treefmt composition contract.
#
# Builds ONE treefmt configuration from per-project treefmt modules so a
# single version-aligned wrapper formats every selected project subtree.
# Project modules supply policy (which formatters, which options, which
# globs); this contract supplies scoping (validated workspace-relative paths,
# unique formatter names) while one shared `pkgs` — plus explicit
# `centralPackages` — supplies one binary per tool for the whole fleet.
#
# Intended caller is fleet orchestration (canix `fleet/format-driver.nix`);
# this file stays pure: project discovery and absolute module paths are
# caller inputs, never baked in here.
{lib}: let
  globChars = ["*" "?" "[" "]" "{" "}" "\\"];

  hasGlobChars = segment:
    lib.any (char: lib.hasInfix char segment) globChars;

  # Validate a workspace-relative project root and return it normalized
  # (leading `./` stripped). Throws on absolute paths, `.`, `..` segments,
  # empty segments, and glob characters — all before anything is formatted.
  # The workspace root itself is expressed as `{ isRoot = true; relPath = "";
  # ... }` and validated separately in compose.
  validateRelPath = name: relPath: let
    stripped =
      if lib.hasPrefix "./" relPath
      then lib.removePrefix "./" relPath
      else relPath;
    segments = lib.splitString "/" stripped;
  in
    assert lib.assertMsg (builtins.isString relPath && relPath != "")
    "treefmt-scope: project ${name} has an empty relPath (non-root projects must name a subtree)";
    assert lib.assertMsg (!lib.hasPrefix "/" relPath)
    "treefmt-scope: project ${name} relPath must be workspace-relative, got ${relPath}";
    assert lib.assertMsg (lib.all (segment:
      segment != "" && segment != "." && segment != ".." && !hasGlobChars segment)
    segments)
    "treefmt-scope: project ${name} relPath is not a clean relative path, got ${relPath}"; stripped;

  slugOf = relPath:
    if relPath == ""
    then "root"
    else lib.replaceStrings ["/"] ["-"] relPath;

  # True when `other` equals `root` or lives beneath it.
  nestsUnder = root: other:
    other == root || lib.hasPrefix (root + "/") other;

  # Prefix a project-local glob with its workspace-relative path, returning
  # a LIST of patterns. The empty relPath (workspace root entry) leaves
  # patterns unchanged.
  #
  # treefmt v2 matches `**/` against one-or-more directories (verified
  # empirically: `a/**/*.nix` misses `a/file.nix` but hits `a/sub/file.nix`),
  # so a bare name like `*.md` must expand to BOTH the direct-child form
  # (`<rel>/*.md`, top-level files) and the nested form (`<rel>/**/*.md`,
  # deeper files). Emitting both is also harmless under gitignore semantics,
  # so excludes use the same expansion.
  scopePatterns = relPath: pattern:
    if relPath == ""
    then [pattern]
    else if pattern == "*"
    then ["${relPath}/**"]
    else if lib.hasInfix "/" pattern
    then
      if lib.hasPrefix "/" pattern
      then ["${relPath}${pattern}"]
      else ["${relPath}/${pattern}"]
    else ["${relPath}/${pattern}" "${relPath}/**/${pattern}"];

  configExtensions = [".toml" ".json" ".yaml" ".yml" ".ini" ".cfg" ".conf"];

  hasConfigExtension = value:
    lib.any (ext: lib.hasSuffix ext value) configExtensions;

  # An option/command element that resolves against the invocation directory
  # instead of a file being formatted. Covers ./ and ../ prefixes, bare
  # config filenames (taplo.toml), and --flag=relative-path spellings.
  # Absolute paths, flags, globs, and bare executable names are fine.
  isRelativeRef = element:
    builtins.isString element
    && (lib.hasPrefix "./" element
      || lib.hasPrefix "../" element
      || element == "."
      || element == ".."
      || isBareConfigFilename element
      || isConfigFlagWithRelativeValue element);

  isBareConfigFilename = element:
    !(lib.hasPrefix "/" element)
    && !(lib.hasPrefix "-" element)
    && !(hasGlobChars element)
    && hasConfigExtension element;

  isConfigFlagWithRelativeValue = element: let
    match = builtins.match "--[A-Za-z0-9_-]+=([^=]+)" element;
  in
    match != null && isRelativeValue (builtins.head match);

  isRelativeValue = value:
    value
    != ""
    && !(lib.hasPrefix "/" value)
    && (lib.hasInfix "/" value || hasConfigExtension value);

  # Scope one evaluated formatter entry beneath relPath, preserving every
  # other key verbatim (options, priority, custom settings).
  scopeFormatter = relPath: formatter:
    formatter
    // {
      includes =
        if formatter ? includes
        then lib.concatMap (scopePatterns relPath) formatter.includes
        else ["${relPath}/**"];
      excludes = lib.concatMap (scopePatterns relPath) (formatter.excludes or []);
    };

  # Option/command elements that resolve against the invocation directory
  # instead of the formatted file silently change meaning when the wrapper
  # runs from the workspace root. Reject them loudly.
  relativeToolRefs = formatter: let
    cmd = lib.toList (formatter.command or []);
    exe =
      if cmd == []
      then null
      else builtins.head cmd;
    rest =
      if cmd == []
      then []
      else builtins.tail cmd;
    exeRefs =
      if
        builtins.isString exe
        && (lib.hasPrefix "./" exe || lib.hasPrefix "../" exe || exe == "." || exe == "..")
      then [exe]
      else [];
  in
    exeRefs ++ lib.filter isRelativeRef (rest ++ (formatter.options or []));

  evalProject = treefmt-nix: pkgs: project: extraModules:
    (treefmt-nix.lib.evalModule pkgs {
      imports = project.modules ++ extraModules;
      projectRootFile = "flake.nix";
      _module.args = project.extraArgs or {};
    }).config;

  enabledPrograms = config:
    lib.filterAttrs (_: program: (program.enable or false))
    (config.programs or {});

  # Central packages win via mkForce, but only for programs the project
  # actually enables — otherwise we would silently ADD formatters.
  # Returns { names, modules }: names get the override so identity reporting
  # can attribute the effective binary without reimplementing each program's
  # mainProgram resolution.
  overridesFor = centralPackages: enabled: let
    names =
      lib.intersectLists (builtins.attrNames centralPackages)
      (builtins.attrNames enabled);
  in {
    inherit names;
    modules = lib.optional (names != []) {
      programs =
        lib.genAttrs names
        (name: {package = lib.mkForce centralPackages.${name};});
    };
  };

  # Baseline command for one program from shared pkgs alone (no project
  # modules). Null when a bare enable does not evaluate — reported honestly
  # instead of guessed.
  baselineCommand = treefmt-nix: pkgs: name: let
    attempt = builtins.tryEval ((treefmt-nix.lib.evalModule pkgs {
        imports = [{programs.${name}.enable = true;}];
        projectRootFile = "flake.nix";
      }).config.settings.formatter.${
        name
      }.command or null);
  in
    if attempt.success
    then attempt.value
    else null;

  # Reference command for one centrally pinned program: the same enable plus
  # the central package, resolved through treefmt-nix's own mainProgram
  # handling. This is what "central" must equal byte-for-byte.
  referenceCommand = treefmt-nix: pkgs: name: package: let
    attempt = builtins.tryEval ((treefmt-nix.lib.evalModule pkgs {
        imports = [
          {
            programs.${name} = {
              enable = true;
              package = lib.mkForce package;
            };
          }
        ];
        projectRootFile = "flake.nix";
      }).config.settings.formatter.${
        name
      }.command or null);
  in
    if attempt.success
    then attempt.value
    else null;

  compose = {
    treefmt-nix,
    pkgs,
    centralPackages ? {},
    # Explicitly tolerated `relPath:tool` divergences, e.g. a project that
    # deliberately pins its own formatter. Anything else flagged
    # `project-override` fails wrapper preparation.
    allowedOverrides ? [],
    # Extra globs appended to the workspace root entry's FORMATTER excludes
    # only (never global): subtrees the root policy must not touch, e.g.
    # unselected project directories. Child coverage is unaffected because
    # these never enter the merged global exclude list.
    rootExtraExcludes ? [],
    projects,
  }: let
    # The workspace root entry (relPath "") carries the root policy and
    # receives every selected child subtree as formatter-level excludes.
    # At most one root entry; it never participates in overlap checks.
    roots = lib.filter (project: (project.isRoot or false)) projects;
    _oneRoot = assert lib.assertMsg (builtins.length roots <= 1)
    "treefmt-scope: at most one isRoot entry is allowed"; true;
    children =
      lib.filter (project: !(project.isRoot or false)) projects;
    normalizedChildren =
      map (project: project // {relPath = validateRelPath project.name project.relPath;})
      children;
    normalizedRoot = map (project:
      assert lib.assertMsg ((project.relPath or "") == "")
      "treefmt-scope: isRoot entry ${project.name} must use relPath \"\""; project)
    roots;
    normalized = normalizedRoot ++ normalizedChildren;

    childRelPaths = map (project: project.relPath) normalizedChildren;

    slugs = map (project: slugOf project.relPath) normalized;
    _slugCheck = assert lib.assertMsg
    (builtins.length (lib.unique slugs) == builtins.length slugs)
    "treefmt-scope: relPath slug collision (e.g. a/b vs a-b, or a project named root)"; true;

    _overlapCheck = assert lib.assertMsg (lib.all (project:
      !(lib.any (other:
        other.relPath
        != project.relPath
        && other.relPath != ""
        && project.relPath != ""
        && nestsUnder other.relPath project.relPath)
      normalized))
    normalized)
    "treefmt-scope: overlapping project roots (duplicate or nested relPath)"; true;

    scopedOne = project: let
      first = evalProject treefmt-nix pkgs project [];
      enabled = enabledPrograms first;
      overrides = overridesFor centralPackages enabled;
      settings =
        if overrides.modules == []
        then first.settings
        else (evalProject treefmt-nix pkgs project overrides.modules).settings;
      formatters = settings.formatter or {};
      _nonempty = assert lib.assertMsg (formatters != {})
      "treefmt-scope: project ${project.name} produced no formatters"; true;
      slug = slugOf project.relPath;
      scoped =
        lib.mapAttrs'
        (name: formatter: lib.nameValuePair "${slug}-${name}" (scopeFormatter project.relPath formatter))
        formatters;
      # Root formatters must not touch selected child subtrees; child
      # formatters own those files. Appended per-formatter (never global),
      # so child coverage is unaffected. The same holds for rootExtraExcludes
      # (unselected subtrees): also per-formatter, never merged globally,
      # otherwise a root exclusion would suppress child rules.
      childExcludes =
        lib.optionals (project.isRoot or false)
        (map (child: "${child}/**") childRelPaths);
      rootExcludes = lib.optionals (project.isRoot or false) rootExtraExcludes;
      # The root entry's own excludes join its formatter excludes as well:
      # nothing from the root policy may enter the merged global exclude
      # list, where it would suppress child rules (e.g. a bare `*.lock`).
      rootOwnExcludes =
        if project.isRoot or false
        then lib.concatMap (scopePatterns project.relPath) (settings.excludes or [])
        else [];
      scopedRooted = lib.mapAttrs (_: formatter:
        formatter
        // {
          excludes = (formatter.excludes or []) ++ childExcludes ++ rootExcludes ++ rootOwnExcludes;
        })
      scoped;
    in
      builtins.seq _nonempty {
        inherit (project) name;
        inherit (project) relPath;
        centralNames = overrides.names;
        formatterNames = builtins.attrNames scopedRooted;
        fragment = {
          settings.formatter = scopedRooted;
          # Evaluated excludes already contain treefmt-nix defaults iff the
          # project left enableDefaultExcludes on; the final eval disables
          # defaults globally, so each project's opt-out survives exactly.
          # The root entry contributes no global excludes (see above).
          settings.excludes =
            if project.isRoot or false
            then []
            else lib.concatMap (scopePatterns project.relPath) (settings.excludes or []);
        };
        # Unscoped policy snapshot for local-vs-global equivalence checks.
        policy =
          lib.mapAttrs (_: formatter: {
            inherit (formatter) command;
            options = formatter.options or [];
            includes = formatter.includes or ["*"];
            excludes = formatter.excludes or [];
          })
          formatters;
      };

    scoped = builtins.seq _oneRoot (builtins.seq _slugCheck (builtins.seq _overlapCheck (map scopedOne normalized)));

    evalResult = treefmt-nix.lib.evalModule pkgs {
      imports =
        (map (entry: entry.fragment) scoped)
        ++ [
          {
            enableDefaultExcludes = false;
            # Project fragments carry no root of their own.
            projectRootFile = "flake.nix";
          }
        ];
    };

    finalNames = lib.concatMap (entry: entry.formatterNames) scoped;
    _finalNameCheck = assert lib.assertMsg
    (builtins.length (lib.unique finalNames) == builtins.length finalNames)
    "treefmt-scope: final formatter name collision"; true;

    baselineNames = lib.unique (lib.concatMap
      (entry: map (formatter: lib.removePrefix "${slugOf entry.relPath}-" formatter) entry.formatterNames)
      scoped);
    baselines = lib.genAttrs baselineNames (baselineCommand treefmt-nix pkgs);
    references =
      lib.mapAttrs
      (name: package: referenceCommand treefmt-nix pkgs name package)
      centralPackages;

    # A centrally pinned tool must equal the central reference exactly.
    # There is no fallback to the shared default: a different binary is a
    # divergence even when it happens to match another known version.
    classify = entry: tool: command:
      if builtins.elem tool entry.centralNames
      then
        (
          if references.${tool} or null != null && command == references.${tool}
          then "central"
          else "project-override"
        )
      else if baselines.${tool} or null != null && command == baselines.${tool}
      then "shared-pkgs"
      else "project-override";

    report = {
      projects =
        map (entry: {
          inherit (entry) name relPath;
          formatters =
            map (scopedName: let
              tool = lib.removePrefix "${slugOf entry.relPath}-" scopedName;
              command = evalResult.config.settings.formatter.${scopedName}.command;
            in {
              name = scopedName;
              inherit tool command;
              source = classify entry tool command;
            })
            entry.formatterNames;
        })
        scoped;
    };

    # Relative tool references would resolve against the workspace root
    # instead of the project directory. Reject before anything is built.
    relativeRefs = lib.concatMap (entry:
      lib.concatMap (scopedName: let
        formatter = evalResult.config.settings.formatter.${scopedName};
      in
        map (ref: {
          formatter = scopedName;
          inherit ref;
        })
        (relativeToolRefs formatter))
      entry.formatterNames)
    scoped;
    _relativeCheck = assert lib.assertMsg (relativeRefs == [])
    "treefmt-scope: relative tool references resolve against the invocation directory, unsupported: ${builtins.toJSON relativeRefs}"; true;

    # Unresolved version divergence fails wrapper preparation. Deliberate
    # exceptions pass through allowedOverrides as "relPath:tool".
    divergences = lib.concatMap (entry:
      lib.concatMap (formatter:
        lib.optional (formatter.source
          == "project-override"
          && !(builtins.elem "${entry.relPath}:${formatter.tool}" allowedOverrides))
        "${entry.relPath}:${formatter.tool} -> ${formatter.command}")
      entry.formatters)
    report.projects;
    _divergenceCheck = assert lib.assertMsg (divergences == [])
    "treefmt-scope: formatter binaries diverge from the aligned toolchain (add explicit allowedOverrides to tolerate): ${builtins.toJSON divergences}"; true;

    _force = builtins.seq _oneRoot (builtins.seq _slugCheck (builtins.seq _overlapCheck
      (builtins.seq _finalNameCheck (builtins.seq _relativeCheck (builtins.seq _divergenceCheck
            (builtins.deepSeq (map (entry: entry.formatterNames) scoped) true))))));
  in
    builtins.seq _force {inherit evalResult report;};
in {
  inherit validateRelPath scopePatterns compose;
}
