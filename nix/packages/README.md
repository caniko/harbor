# Companion package provenance

Blender MCP and OpenPencil packaging was extracted from Canix at
`71ace3dce49c5f4c52db5c49879796b48bb19409`. Original paths:

- `root/packages/blender_mcp/`
- `home/modules/productivity/blender.nix` and `blender-mcp-startup.py`
- `pkgs/open-pencil-cli/` and `pkgs/open-pencil-desktop/`
- `flake/checks/open_pencil.nix`
- `scripts/test-blender-mcp.py`

Pinned upstream sources, package identities, licenses and imported
behavioral patches are preserved. Imported dependency locks and patch files
retain their exact bytes. The Blender extension's persisted `canix` repository
identity is retained. Canix's host/GPU selection stays in the consumer adapter.

OpenPencil remains CLI 0.15.1 and desktop 0.15.1-canix1. The desktop's export
patch and isolated native editing/export/reopen acceptance belong to this shared
package owner. See `docs/tooling-modules.md` for module selection and delivery.

The dependency archive now requires two cold frozen Bun installs to agree after
repairing the pinned linker's two nondeterministic links. Blender starts its
bridge explicitly and uses actual GPU offscreen viewport capture when Mesa's
llvmpipe front-buffer read returns black under Xvfb. The native acceptance gate
requires nonblank pixels, graph edits and disposable-file save/reopen.
