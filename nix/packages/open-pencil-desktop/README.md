# OpenPencil desktop package

The desktop builds from upstream 0.15.1 at
`6cf1748e31a0e43d3794d43abbd003ce9caa3c6f`. The CLI/MCP package under
`../open-pencil-cli` uses the matching published release. The desktop wrapper
puts that CLI on `PATH` so launching from the desktop menu finds the same MCP
version as launching from a terminal.

`document-export.patch` fixes canvas ownership during tab teardown and raster
exports from inactive documents or non-current pages. Each export captures its
page before awaiting fonts, uses an independent renderer, and releases that
renderer on success or failure. It preserves font metadata and captured layout.
Missing fonts can use fallback glyphs; visible text does not establish exact
typographic fidelity.

`frontend.nix` requires two independent cold, frozen Bun installs to produce
byte-identical dependency archives in its fixed-output derivation. The pinned
Bun 1.4.2 linker races on the shared `react-is` alias and ESLint/Browserslist peer binaries;
`normalize-links.ts` binds the alias to the unaliased frozen-lock entry and
recreates the binaries from their already-resolved peer manifests. The Browserslist
peer identity and bin path must match the frozen lock. Its missing-bin regression
also runs inside the fixed-output derivation. Package payloads,
workspace links, and version-specific dependency links retain their contents.
The archive stores regular-file bytes independently of Bun's filesystem-dependent
hardlink sharing and uses numeric owners. Symbolic workspace and dependency links
remain intact. Backend qualification compares the logical graph and archive bytes
across hardlink and copyfile installs; both independent cold installs must still
agree inside the fixed-output derivation.
The offline build extracts the archive, patches
shebangs, builds packages and native assets, and runs real CanvasKit export
regressions. Missing remote fonts in those tests are simulated at the font-fetch
boundary so the sandbox never needs a public font catalog.

## Verification

Run from the Harbor root:

```sh
canix cache binary build .#open-pencil-desktop --no-push --cores 4
canix cache binary build .#checks.x86_64-linux.open-pencil-package-smoke --include-tests --no-push --cores 4
canix cache binary build .#checks.x86_64-linux.open-pencil-desktop-mcp --include-tests --no-push --cores 4
```

The smoke check inspects the native ELF, desktop entry, icon, and CLI FIG
round-trip. The native check uses isolated homes, runtime directories, session
buses, Xvfb, and Mesa surfaceless EGL. It requires MCP editing, nonblank text
exports, inactive-document/non-current-page targeting, save, and a fresh
desktop restart that reads and renders the saved text.

The patch also includes the browser regression
`tests/e2e/export/document-target.spec.ts`. In a prepared upstream checkout:

```sh
env -u VK_DRIVER_FILES -u MESA_VK_DEVICE_SELECT \
  PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH=/path/to/chrome-headless-shell \
  OPENPENCIL_TEST_PORT=1436 OPENPENCIL_TEST_MCP_PORT=7616 \
  bun run test tests/e2e/export/document-target.spec.ts --timeout=60000
```

Atlas's inherited Radeon Vulkan selection prevents Chromium's SwiftShader from
creating WebGL contexts. Clear it for this software-rendered test process only.
The test uses upstream's canonical Playwright server and isolated MCP discovery.

Builds do not activate Home Manager or replace a running app. Save open documents
before restarting with the new desktop. Existing agent sessions may retain the
previous stdio bridge and need reconnecting after the CLI/MCP upgrade.
