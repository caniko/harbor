# Dioxus packages

`rs-harbor.lib.mkDioxusWebPackage` and
`rs-harbor.lib.mkDioxusFullstackPackage` provide the shared Nix mechanics for
Dioxus 0.7 applications. They vendor Cargo dependencies, resolve the exact
`wasm-bindgen-cli` version recorded in `Cargo.lock`, add the native linker
tools needed by fullstack builds, and run the Dioxus CLI offline.

## Web package

Use the web builder when the product already owns the server process or static
asset routing:

```nix
rs-harbor.lib.mkDioxusWebPackage {
  inherit pkgs craneLib rustToolchain;
  src = filteredSource;
  cargoLock = ./Cargo.lock;
  pname = "my-app-dioxus";
  package = "my-app";
  wasmBindgenCli = exactWasmBindgenCli;
  webFeatures = [ "web" ];
  wasmSplit = true;
  installSubdir = "share/my-app/dioxus";
}
```

The derivation installs the generated Dioxus `public/` tree below
`installSubdir` (including `index.html`, hashed JavaScript, and WASM assets).
The product can copy that tree into its own static directory and apply its own
compression or cache policy.

Release plans pass `--debug-symbols=false` explicitly. Dioxus 0.7 otherwise
retains DWARF even for `--release`, increasing the browser artifact and making
some Binaryen releases abort while re-emitting debug information. Non-release
profiles keep symbols by default; set `debugSymbols` explicitly when a product
needs the opposite policy.
The builder also turns an ignored `wasm-opt` failure back into a failed Nix
build, preventing an unoptimized fallback bundle from reaching a release.

## Fullstack package

Use the fullstack builder when Dioxus owns the deployable server executable:

```nix
rs-harbor.lib.mkDioxusFullstackPackage {
  inherit pkgs craneLib rustToolchain;
  src = filteredSource;
  cargoLock = ./Cargo.lock;
  pname = "my-app";
  package = "my-app";
  wasmBindgenCli = exactWasmBindgenCli;
  webFeatures = [ "web" ];
  serverFeatures = [ "server" ];
  publicSubdir = "share/my-app/public";
}
```

The output contains `bin/my-app` and `bin/my-app-unwrapped`, plus the generated
public tree. The wrapper sets `DIOXUS_PUBLIC_PATH` to the packaged public path;
set `wrapServer = false` when the product supplies its own process wrapper.

## Externally built servers

Some applications own an Axum or other server executable that embeds Dioxus
SSR while building the browser bundle separately. Add the asset linker to that
server derivation's native build inputs and run it before wrapping or stripping
the executable:

```nix
let
  dioxusAssetLinker = rs-harbor.lib.mkDioxusAssetLinker {
    inherit pkgs;
    dioxusCli = pkgs.dioxus-cli;
  };
in
craneLib.buildPackage {
  nativeBuildInputs = [ dioxusAssetLinker ];
  postInstall = ''
    dioxus-link-assets \
      "$out/bin/my-server" \
      "$out/share/my-app/public/assets"
  '';
}
```

This runs Dioxus' own `dx tools assets` command, which copies declared assets
and patches their hashed paths into the executable. Crane consumers must call
it in the derivation's `postInstall`, before reference-removal hooks sanitize
vendored source paths.

## Toolchain and feature policy

`mkDioxusBuildPlan` is available for consumers that need to inspect or compose
the command shape. Fullstack plans build the client with `@client` and the
server with `@server --server`, allowing independent Cargo feature and target
arguments. `resolveWasmBindgenCli` fails early when the lockfile version has no
exact nixpkgs package; callers may pass a custom derivation with the matching
`.version` instead. `mkDioxusPackage` is retained as a compatibility alias for
the web builder during the migration window.
