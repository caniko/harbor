# tex-harbor

`tex-harbor` provides reusable Nix helpers for TeX and LaTeX projects. It
centralizes TeX Live profiles and the reproducible parts of PDF compilation
while leaving manuscript layout, source trees, engine policy, and publication
artifacts with each consumer.

The flake exports packages and checks for `x86_64-linux`, `aarch64-linux`,
`x86_64-darwin`, and `aarch64-darwin` with its current Nixpkgs pin. Its `lib`
output is system independent and can be consumed by projects that still expose
other systems.

The anx plugin packages are canonical here:

- `packages.<system>.anx-plugin-zenodo` — the Rust Zenodo archival plugin.
- `packages.<system>.anx-plugin-pandoc` — the Python Pandoc ODT export plugin.

Their sources and manifests live under `plugins/`; the package checks include a
Zenodo build and a Pandoc import smoke test.

## Profiles

- `cv` — small pdfLaTeX documents such as CourseOfLife.
- `article` — scientific LuaLaTeX/pdfLaTeX documents with bibliography, SVG,
  TikZ, math, and reference tooling.
- `conference` — curated dependencies for conference templates.
- `editor` — interactive tooling for the Canix workstation environment.

## Consumer example

```nix
inputs.tex-harbor.url = "git+https://codeberg.org/caniko/tex-harbor.git?ref=trunk";
inputs.tex-harbor.inputs.nixpkgs.follows = "nixpkgs";

texlive = tex-harbor.lib.mkTexlive {
  inherit pkgs;
  profile = "article";
};

article = tex-harbor.lib.mkLatexDocument {
  inherit pkgs;
  name = "article";
  src = ./article;
  workingDirectory = ".";
  mainFile = "manuscript.tex";
  engine = "lualatex";
  profile = "article";
  shellEscape = true;
  nativeBuildInputs = [ pkgs.inkscape ];
};
```

`workingDirectory` may point below a larger source tree, which is useful when
an article depends on generated assets or shared TikZ sources elsewhere in a
repository. `preBuild` and `postBuild` are available for consumer-specific
validation such as page-count limits; publication policy stays in the consumer.

Project-specific `.latexmkrc` files remain valid. They are the right place for
publication-specific entry points, BibTeX/Biber policy, and shell-escape
decisions that cannot be shared safely across documents.

## Development

```bash
nix flake init -t git+https://codeberg.org/caniko/tex-harbor.git
nix flake check
nix develop
```
