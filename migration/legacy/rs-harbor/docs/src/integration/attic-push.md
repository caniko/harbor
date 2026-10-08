# mkAtticPush

`mkAtticPush` creates a Nix app that pushes store paths to Attic using an ephemeral credential file. It can publish explicit paths, optionally including their closures, and every recursively locked source path returned by `nix flake archive`.

## Example

```nix
apps.push-cache = rs-harbor.lib.mkAtticPush {
  inherit pkgs;
  adapter = infra.harborAdapter;
  paths = [ self.packages.${system}.default ];
};

apps.push-flake-inputs = rs-harbor.lib.mkAtticPush {
  inherit pkgs;
  adapter = infra.harborAdapter;
  flake = ".";
};
```

Run it with:

```bash
nix run .#push-cache
nix run .#push-flake-inputs
```

The generated script expects the token environment variable described by the adapter, defaulting to `ATTIC_TOKEN`. It never runs `attic login`; its mode-0600 Attic configuration is removed on exit. Set `HARBOR_ATTIC_MANIFEST` to retain the sorted path manifest outside the temporary directory.
