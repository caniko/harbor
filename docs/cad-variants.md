# Controlled CAD-copy variants

`cad variant REQUEST.json` and CAD/all MCP `cad_plan_variant` prepare a distinct
version-16 approval for one registered, originally authorized CAD document.
Ordinary `job submit` executes the approved variant through the patched,
closure-only FreeCAD importer sandbox. Preparation opens no CAD document.

The first allowlist accepts a document containing exactly one top-level native
`Part::Box`, identified by its registered object name, with no expressions,
linked objects or assembly context. Explicit length/width/height quantities,
geometry tolerance and provenance are required. The original axis-aligned
placement is preserved. Native execution checks the original dimensions, world
bounds and volume against registered source evidence before changing the
allowlisted dimension properties, recomputes, rechecks all named geometry and
exports a new FCStd, BREP and controlled STL. Face numbering and healing are
never used as identity.

Original document bytes and the original science/execution/binding/authorization
identities are bound in the new approval. Distinct-inode verified source copies
and provenance are retained before submission acknowledgment, under independent
shared staging admission. Only the copy is opened; the source is mounted
read-only and native execution verifies its checksum again after saving the new
document. Interrupted unpublished staging is recovered only while the original
registered source remains verifiable; otherwise originals remain quarantined.

Rust reconstructs expected volume, world bounds and unchanged placement from
the original source and explicit dimensions. It checks the native original and
recomputed region receipts before publication and repeats registered-byte checks
for historical qualification. Native geometry execution does not constitute a
physics solve, mesh convergence or physical validation. Assemblies, rotations,
arbitrary object properties, spreadsheet formulas and multi-solid variants are
outside this initial allowlist.

`examples/cad-variant.json` is an explicit request template. Replace its all-zero
source UUID with an authorized single-box inspection job; dimensions never
change automatically to fit a resource limit. Source CPU contract/allowlist and
staging tests are independent of native package and lifecycle qualification.

The native allowlist follows the pinned FreeCAD
[`Part::Box` implementation](https://github.com/FreeCAD/FreeCAD/blob/4fd3bf320d9566a27e60069fc8387448aaa3a094/src/Mod/Part/App/FeaturePartBox.cpp):
`Length`, `Width` and `Height` are native properties, recomputation builds an
OpenCASCADE box from their values and the native primitive preserves placement.
The importer uses the same pinned document/placement APIs as the qualified
original import. World-space region and BREP semantics are rechecked after the
edit rather than relying on native property names alone.

`scripts/verify_cad_variant_worker.py` is the opt-in exact-package gate. It
generates pinned native origin/translated boxes, inspects their originals,
checks CLI/MCP planning parity and runs controlled copies after independent
approval. Pre-submit substitution is refused; acknowledged original copies
survive subsequent source mutation and a worker crash under the same service
invocation. New documents are opened by a second independent patched import to
check saved geometry/placement, followed by owned cancellation/forced-death
and reservation/runtime-root release. Development packages are refused before
creating a campaign output. This gate does not pass until executed successfully.
