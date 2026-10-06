# Imported CAD mesh correspondence

The initial BREP importer allowlist requires top-level native `Part::Feature`
solids. Links and solids inside geometric groups reject before exports, because
their local placement/bounds need a separately verified world-transform
traversal. FreeCAD's pinned
[`DocumentObjectPyImp.cpp` getParentGeoFeatureGroup implementation](https://github.com/FreeCAD/FreeCAD/blob/4fd3bf320d9566a27e60069fc8387448aaa3a094/src/App/DocumentObjectPyImp.cpp#L801)
returns the enclosing group or `None`; this is the exact API used for the gate.

The patched sandbox importer now exports each approved named solid as a closed
BREP plus a byte/hash/unit/placement manifest. The original FCStd remains
read-only. The original named-region JSON schema is preserved; BREP metadata is
in the separate `brep-manifest.json`. These outputs add geometry correspondence
evidence and do not establish a numerical solve.

`cad-mesh-cpu` is an independent isolated Gmsh 4.15.2/Python 3.13 adapter for
one imported axis-aligned box. `runtime-cad-mesh-cpu` records its exact executable
and bubblewrap launcher. Its explicit descriptor binds the BREP byte count and
SHA-256, approved named region, world bounds/volume, synthetic provenance,
original source placement and millimetre units. Refinement, geometric tolerance
and the maximum `1e-10` volume-correspondence gate are explicit. There is no gap
healing, shape reconstruction or automatic inference of a more general model.

The adapter imports exactly one solid using OCC `importShapes`, forces BREP
format and scales coordinates by the declared `0.001` around the global origin.
World placement is preserved in SI. It verifies the imported volume/world bounds,
eight vertices, twelve straight edges and six planar faces. Semantic faces use
geometric bounds and area checks, without relying on ordinal OCC tags. Structured
C3D8 generation, positive Gauss-point Jacobians, mesh-volume conservation and
complete boundary nodes reuse the established FEM mesh verifier.

## Source-backed API decisions

- FreeCAD 1.1.4 `TopoShapePy::exportBrep` writes the selected shape without
  rebuilding its geometry: <https://github.com/FreeCAD/FreeCAD/blob/4fd3bf320d9566a27e60069fc8387448aaa3a094/src/Mod/Part/App/TopoShapePyImp.cpp#L396>.
- Gmsh 4.15.2 `api/gmsh.py` inspected in the exact package defines
  `model.occ.importShapes(fileName, highestDimOnly=True, format="")`, documented
  at <https://gmsh.info/doc/texinfo/gmsh.html#gmsh_002fmodel_002focc_002fimportShapes>.
- The same immutable module defines `model.occ.dilate(dimTags,x,y,z,a,b,c)`:
  <https://gmsh.info/doc/texinfo/gmsh.html#gmsh_002fmodel_002focc_002fdilate>.
  Its documented center and independent axes are supplied explicitly; source
  millimetres and matrix-translation units are retained beside SI coordinates.

Source versions/hashes/ABI remain those in [FEM references](fem-references.md)
and [dependency manifest](dependency-manifest.json). The initial allowlist
rejects curved, multi-solid, ambiguous or mismatched imported geometry.
Native correspondence qualification and durable imported-FEM planning are
separate remaining gates; static synthetic and transient packages inherit no
qualification from this new mesh/importer code.

`scripts/verify_cad_mesh.py` uses `runtime-cad-fixtures` to generate only two
fixed controlled source documents: an origin box and a box translated by
`(100, -20, 300) mm`. It imports each through an authority-approved CAD-only
worker service before meshing its exact exported BREP at resolutions 2/4/8.
An independent verifier checks the complete Cartesian world grid, oriented
cell volumes, non-overlapping cell coverage and all six semantic face sets.
It retains sources, approvals, jobs, checksummed exports and complete native
commands. Wrong bytes/units/gates/bounds/paths, multiple solids and curved
geometry must fail before publishing a mesh receipt. Fixed negative BREP
fixtures are created through the same immutable security-patched FreeCAD ABI;
they are qualification inputs, with synthetic provenance retained.
