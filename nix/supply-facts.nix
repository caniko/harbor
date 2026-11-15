# Harbor's producer facts for `fleetix supply` (contract fleetix.supply-facts.v1).
#
# One `producer:` node per `.gitmodules` component, with a `memberOf` edge to
# the superproject, so adding or removing a submodule changes the facts. The
# alias table folds the pre-rename names and Codeberg mirrors that consumers
# still pin into each component's GitHub coordinate.
#
# Pure data, no nixpkgs: `import "${harbor}/nix/supply-facts.nix" { inherit
# (harbor) rev lastModifiedDate; }` from a `flake = false` input.
{
  rev ? "unknown",
  lastModifiedDate ? "19700101000000",
}: let
  superproject = "github.com/caniko/harbor";
  # Old repository names, per current GitHub repository name.
  renamed = {
    harbor-meta = ["meta-harbor"];
    harbor-rs = ["rs-harbor"];
    harbor-py = ["py-harbor"];
    harbor-js = ["js-harbor"];
    harbor-macos-sdk-pin = ["rs-harbor-macos-sdk-pin"];
  };
  aliasesOf = coordinate: let
    name = baseNameOf coordinate;
  in
    builtins.filter (alias: alias != coordinate) (builtins.concatMap
      (old: ["github.com/caniko/${old}" "codeberg.org/caniko/${old}"])
      ([name] ++ renamed.${name} or []));

  lines = builtins.filter builtins.isString (builtins.split "\n" (builtins.readFile ../.gitmodules));
  # [{path, url, branch}] in file order; a header starts a new entry.
  submodules =
    builtins.foldl' (
      acc: line: let
        header = builtins.match ''\[submodule "(.*)"]'' line;
        field = builtins.match ''[[:space:]]*(path|url|branch) = (.*)'' line;
        last = builtins.length acc - 1;
      in
        if header != null
        then acc ++ [{}]
        else if field != null && acc != []
        then
          (builtins.genList (i: builtins.elemAt acc i) last)
          ++ [(builtins.elemAt acc last // {${builtins.elemAt field 0} = builtins.elemAt field 1;})]
        else acc
    ) []
    lines;
  coordinateOf = url: let
    stripped = builtins.match "https://(.+)\\.git" url;
  in
    if stripped != null
    then builtins.head stripped
    else builtins.head (builtins.match "https://(.+)" url);

  producer = coordinate: defaultBranch: {
    id = "producer:${coordinate}";
    kind = "producer";
    attrs = {
      inherit defaultBranch;
      aliases = aliasesOf coordinate;
    };
  };
  components = map (s: s // {coordinate = coordinateOf s.url;}) submodules;
  d = lastModifiedDate;
  sub = builtins.substring;
in
  assert builtins.all (s: s ? path && s ? url && s ? branch) submodules;
  assert builtins.length submodules == builtins.length (builtins.filter (l: builtins.match ''\[submodule .*'' l != null) lines); {
    contract = "fleetix.supply-facts.v1";
    vocabulary = "harbor.supply-facts.v1";
    producer = {
      name = "harbor";
      version = rev;
    };
    observedAt = "${sub 0 4 d}-${sub 4 2 d}-${sub 6 2 d}T${sub 8 2 d}:${sub 10 2 d}:${sub 12 2 d}Z";
    nodes =
      [
        (producer superproject "feat/harbor-monorepo-components")
        # Vendored at components/macos-sdk, not a submodule, still pinned directly.
        (producer "github.com/caniko/harbor-macos-sdk-pin" "trunk")
      ]
      ++ map (c: producer c.coordinate c.branch) components;
    edges =
      map (c: {
        from = "producer:${c.coordinate}";
        to = "producer:${superproject}";
        kind = "memberOf";
        attrs.path = c.path;
      })
      components;
  }
