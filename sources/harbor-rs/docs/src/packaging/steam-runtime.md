# Steam Runtime Tools

`mkSteamRuntimeTools` provides generic helpers for Rust games that ship native
Linux builds on Steam and want explicit dynamic-linking audits.

The helpers deliberately avoid app-specific policy. Your project still owns
Steam app IDs, depot layout, release branches, crash handlers, symbols, and the
actual Cargo or engine build command.

## Basic Usage

```nix
steamRuntimeTools = harbor-rs.lib.mkSteamRuntimeTools {
  inherit pkgs;
};
```

The default runtime is Steam Linux Runtime 3.0 `sniper`:

```nix
steamRuntimeTools.image
# "registry.gitlab.steamos.cloud/steamrt/sniper/sdk"
```

You can select another known runtime or provide a custom OCI image:

```nix
steamRuntimeTools = harbor-rs.lib.mkSteamRuntimeTools {
  inherit pkgs;
  runtime = "scout";
};
```

```nix
steamRuntimeTools = harbor-rs.lib.mkSteamRuntimeTools {
  inherit pkgs;
  runtime = "my-runtime";
  customImage = "registry.example.com/my/steam-sdk:latest";
};
```

## Flake Apps

Expose the tools as apps so `just`, CI, and release scripts can call them:

```nix
apps = {
  steam-runtime-exec = {
    type = "app";
    program = "${steamRuntimeTools.steamRuntimeExec}/bin/steam-runtime-exec";
  };

  audit-elf-runtime-deps = {
    type = "app";
    program = "${steamRuntimeTools.auditElfRuntimeDeps}/bin/audit-elf-runtime-deps";
  };
};
```

## Running Commands In The SDK Container

`steam-runtime-exec` runs a command in the selected SDK image, mounting the
current working directory at the same path:

```sh
nix run .#steam-runtime-exec -- -- cargo build --release
```

The wrapper does not install Rust, Nix, engine dependencies, or Steamworks SDK
content. Projects with custom toolchains should either bake those into their own
image or use the wrapper for runtime validation commands.

For Nix-built audit tools, mount `/nix/store` read-only:

```sh
nix run .#steam-runtime-exec -- --mount-nix-store -- \
  /nix/store/.../bin/audit-elf-runtime-deps dist/linux
```

## Dependency Audits

The audit scripts are intentionally allowlist-driven:

```sh
nix run .#audit-elf-runtime-deps -- \
  --require-origin-rpath \
  --allow-needed-regex '^(lib(c|m|dl|pthread|gcc_s|stdc\+\+|steam_api).*)\.so(\..*)?$' \
  dist/linux
```

Windows and macOS use the same pattern:

```sh
nix run .#audit-windows-runtime-deps -- \
  --allow-dll-regex '^(KERNEL32|USER32|ADVAPI32|WS2_32|steam_api64)\.dll$' \
  dist/windows
```

```sh
nix run .#audit-darwin-runtime-deps -- \
  --allow-dylib-regex '^(@executable_path|@rpath|/usr/lib/|/System/Library/)' \
  dist/macos
```

For Linux Steam release candidates, run the ELF audit inside the same Steam
Runtime container that the Steamworks launch option selects.
