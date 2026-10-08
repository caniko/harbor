# Imported CAD mesh correspondence

The initial BREP importer allowlist requires top-level native `Part::Feature`
solids. Links and solids inside geometric groups reject before exports, because
their local placement/bounds need a separately verified world-transform
traversal. FreeCAD's pinned
[`DocumentObjectPyImp.cpp` getParentGeoFeatureGroup implementation](https://github.com/FreeCAD/FreeCAD/blob/4fd3bf320d9566a27e60069fc8387448aaa3a094/src/App/DocumentObjectPyImp.cpp#L801)
returns the enclosing group or `None`; this is the exact API used for the gate.

`src/cad_source.rs` resolves a selected solid exclusively from a succeeded,
originally authorized CAD job. Its versioned source descriptor binds original
science/execution/authorization identities, the closed BREP and region
manifest hashes, named world bounds, original millimetre placement and exact
BREP bytes. Source resolution rechecks registered evidence, source units and
the patched importer/closure policy. It does not reopen the document, accept
caller-supplied replacement bounds, heal gaps or qualify a solve.

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
- Import uses `Geometry.OCCScaling=0.001` with all five OCC repair/sewing/solid
  construction options explicitly disabled. In pinned
  [`GModelIO_OCC.cpp`](https://github.com/live-clones/gmsh/blob/gmsh_4_15_2/src/geo/GModelIO_OCC.cpp#L4886),
  import calls `_healShape` for uniform `gp_Trsf` scaling, then returns without
  healing when all repair flags are zero
  ([lines 5886–5901](https://github.com/live-clones/gmsh/blob/gmsh_4_15_2/src/geo/GModelIO_OCC.cpp#L5886)).
  Generic `occ.dilate` uses `gp_GTrsf` instead and converts analytic lines/planes
  into trimmed curves/B-spline surfaces even for equal scale factors. It is not
  used for source-unit normalization. Source millimetres and matrix-translation
  units remain beside SI coordinates.

Source versions/hashes/ABI remain those in [FEM references](fem-references.md)
and [dependency manifest](dependency-manifest.json). The initial allowlist
rejects curved, multi-solid, ambiguous or mismatched imported geometry.
Native correspondence passes at the exact repaired package described in
[imported CAD/FEM evidence](evidence/imported-cad-fem-cpu.json). Durable
imported-FEM planning remains a separate gate; static synthetic and transient
packages inherit no qualification from this new mesh/importer code.

The first exact CAD correspondence gate, `cad-mesh-1` at `4e23c6d`, correctly
rejected the origin box before meshing because generic dilation had changed its
analytic topology. Independent exact-ABI diagnosis retained the original BREP
digest and proved that uniform import scaling preserves 12 lines and 6 planes
at the same SI volume. The failed attempt and diagnosis remain separate from
the repaired-package gate; line/plane, face-area, volume and mesh checks are
unchanged.

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

`cad-mesh-2` passed both placements at resolutions 2/4/8 and all seven negative
cases. Maximum independently checked relative volume error was
`9.366066814264428e-13`, below the unchanged `1e-10` gate; the aggregate campaign
RAM peak was `109367296` bytes under two CPUs, 2 GiB, no swap and 128 tasks.

## Durable source-bound meshing

`harbor-cad --socket SOCKET cad mesh REQUEST.json` and CAD-profile MCP
`cad_plan_mesh` resolve one registered authorized source job/region. The request
contains `source_job`, `region_name`, `resolution` and `geometry_tolerance_m`.
The returned version-7 plan and approval digest can be submitted through
`job submit` or `cad_mesh_submit`, which returns a durable job ID.

The plan has its own CAD source identity and no fluid/FEM/thermal inputs or
invented physical times. Prior versions reject injected CAD sources, including
null fields. Submission reserves source staging capacity and publishes verified
distinct-inode BREP/manifest/region copies before acknowledgment. Durable
staging intents preserve incomplete copies if the original authorized source
has been lost; valid original evidence permits orphan recovery. Source bytes
are checked before and after the native operation.

`runtime-cad-mesh-worker` mounts only its immutable Gmsh/Python operation closure
and the selected retained BREP as read-only input. It requires authoritative
admission, owned systemd services and effective CPU/RAM/task/disk controls.
Receipts verify package/source/unit/world-placement/mesh identities and ten
isolation checks. Rust independently checks complete Cartesian nodes, cell
coverage, all eight C3D8 Gauss Jacobians per cell, conserved volume and all six
boundary node sets before publishing the stage. Geometric correspondence stays
distinct from a field solve, convergence study or physical validation. The
version-7 native worker/export/lifecycle gate passed as `cad-mesh-worker-2`.
[Exact worker evidence](evidence/cad-mesh-worker-cpu.json) binds build 22 packages
and the independently captured qualifier. CLI and pinned MCP n8 jobs retained
20 artifacts each and measured service-tree RAM peaks of `35950592` and
`36261888` bytes. The failed launcher-only first attempt remains recorded;
the fresh campaign passed all source mutations, restart/idempotency, export,
forced-tree death, cancellation and final reservation/root release checks.
