# harbor-projects

Reusable mdBook + plinth-project documentation infrastructure for Nix flakes.

`harbor-projects` owns the boring docs plumbing so consuming flakes keep project
policy local: book layout, build derivation, plinth `docsPackage` merge,
dev shell, and doc checks live here; titles, content, and domains stay with
each consumer.

## Usage

```nix
{
  inputs.harbor-projects.url = "github:caniko/harbor-projects";
}
```

```nix
# nix/site.nix
{
  pkgs,
  projectSiteLib,
  harborProjects,
}: {
  docs = harborProjects.mkDocs {
    inherit pkgs;
    src = ../docs;
    pname = "my-project-docs";
  };

  site = harborProjects.mkSite {
    inherit pkgs projectSiteLib;
    domain = "my-project.tartanoglu.com";
    configPath = ../website/plinth-project.toml;
    docs = self.docs; # via packages wiring
    staticPaths = [
      {
        source = ../website/static/my-mark.svg;
        target = "website/static/my-mark.svg";
      }
    ];
  };
}
```

Or scaffold a new docs tree:

```bash
nix flake init -t github:caniko/harbor-projects
```

## API

- `mkBookToml { title, repo, siteUrl, authors ?, src ? }` — opinionated
  `book.toml` text (coal dark theme, `edit-url-template` to
  `.../edit/trunk/docs/{path}`).
- `mkDocs { pkgs, src, bookToml ?, pname ?, version ? }` — `mdbook build`
  derivation; `$out` is the built book. Pass `bookToml` (a store path) to
  override the consumer's `docs/book.toml`.
- `mkSite { projectSiteLib, domain, configPath, staticPaths ?, docs, pname ?, version ?, pkgs ?, appSource ?, appDir ?, acceptance ? }` —
  thin wrapper over `mkProjectSite { docsPackage = docs; }` so the built book
  lands at `$out/docs/`. Pass `pkgs` + `appSource` (built web root,
  `index.html` at top) to also embed the app at `$out/<appDir>/`
  (default `app`, so landing CTAs point at `/app`). The bundle must emit
  subpath-safe URLs — for Dioxus, build it with `Dioxus.toml`
  `[web.app] base_path = "<appDir>"` so `dx` prefixes asset links and
  bakes the prefix into the wasm router.
  Embedded apps require `acceptance = { command; requiredTests; ...; }`, using
  `harbor-meta.packageTests.mkCheckedArtifact`. The command receives the assembled
  site as `HARBOR_ARTIFACT` and must write its normalized test report to
  `HARBOR_ACCEPTANCE_REPORT`. Pass matching backend packages through
  `acceptance.artifacts.backend` and runtime tools through
  `acceptance.nativeBuildInputs`. The returned deployment output is only produced
  after every required test passes; `.harbor/acceptance.json` records test IDs,
  tested store paths and SHA-256 file inventories. Existing embedded-app callers
  must add an artifact-bound acceptance runner when updating this library.
  `packageTests.mkArtifactVerifier { inherit pkgs; }` provides a read-only local
  and HTTP identity verifier for deployment hooks.
- `mkWebsiteMarkers { pkgs, website, title, appRoute ?, sections ?, extraGreps ?, name ? }` —
  fails when the built site's `index.html` misses the title, the
  `href="<appRoute>"` funnel link, or the rendered section markers
  (default `workflow-steps`, `audience-grid`, `trust-panel`).
- `mkDocsDevShell { pkgs, plinthProject, extraPackages ? }` — `mdbook` +
  `plinth-project` shell (`plinthProject` comes from the consumer's plinth
  input; harbor-projects does not pin plinth itself).
- `mkSummaryCheck { pkgs, src }` — fails when a `docs/src/*.md` file is not
  reachable from `SUMMARY.md`.
