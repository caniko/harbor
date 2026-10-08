# mkAppImage

`mkAppImage` wraps a built Linux executable into a self-contained AppImage.

## Notes

- Linux only
- Opt-in: you must add `nix-appimage` to your own flake inputs
- The result is a derivation that emits a `.AppImage` file

## Example

```nix
packages.appimage = rs-harbor.lib.mkAppImage {
  inherit system nix-appimage;
  program = "${myPackage}/bin/my-app";
};
```

Use this when you already have a reproducible Nix build and want a portable desktop artifact on top of it.
