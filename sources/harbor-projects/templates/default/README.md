# my-project docs

Scaffolded with `nix flake init -t github:caniko/harbor-projects`.

Replace `my-project` / `caniko/my-project` / `my-project.tartanoglu.com`
in `flake.nix`, `nix/*.nix`, `site/flake.nix`, `simit.toml`,
`website/plinth-project.toml`, and `docs/book.toml`, then run
`simit sync` to generate CI workflows.

- Docs live in `docs/` (`mdbook serve docs` from the dev shell).
- The project site is published from `site/` (`./site#site`).
