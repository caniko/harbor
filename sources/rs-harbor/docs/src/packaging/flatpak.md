# mkFlatpakManifest

`mkFlatpakManifest` generates a Flatpak manifest JSON file for packaging a pre-built application with `flatpak-builder`.

## Parameters

- `pkgs`
- `appId`
- `pname`
- `desktopFile`
- `icon`
- `runtime`
- `runtimeVersion`
- `sdk`
- `sdkExtensions`
- `finishArgs`

## Example

```nix
packages.flatpak-manifest = (rs-harbor.lib.mkFlatpakManifest {
  inherit pkgs;
  appId = "com.example.MyApp";
  pname = "my-app";
  desktopFile = ''
    [Desktop Entry]
    Type=Application
    Name=My App
    Exec=my-app
    Icon=com.example.MyApp
    Categories=Utility;
  '';
}).manifestPath;
```

This is useful when your project already builds the binary in Nix, but you want Flatpak metadata generated from the same source of truth.
