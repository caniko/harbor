# Bevy Template

`harbor-rs` exports a `bevy` flake template for new game projects.

## Initialize from the template

```bash
nix flake init -t git+https://github.com/caniko/harbor-rs.git#bevy
```

## What the template includes

- `mkToolchain`, `mkCross`, and `mkCargoConfig` wired into the project flake
- `mkDevShells` for native, Windows, macOS, and combined cross shells
- commented examples for `mkAppImage` and `mkFlatpakManifest`
- Bevy-specific shell dependencies in `nix/dev-shells.nix`

If you want to understand the generated shell behavior in more detail, read [mkDevShell and mkDevShells](../api/dev-shells.md).
