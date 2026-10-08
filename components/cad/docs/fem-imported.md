# Imported box static FEM references

`fem-imported-cpu` continues the [CAD mesh correspondence](cad-mesh.md) slice
through independent native CalculiX 2.23/SPOOLES conduction and free-expansion
references. `runtime-fem-imported-cpu` is a standalone exact-runtime descriptor;
authority-backed imported FEM worker planning remains a separate integration
gate. The geometry/BREP, reference parameters, explicit material provenance and
boundary provenance are distinct strict request fields. They must agree on
synthetic status, SI box dimensions, mesh refinement and geometric tolerance.
No missing material property is substituted and the maximum numerical gate
remains the unchanged static `1e-6`.

The solver consumes native world-space nodes from the imported BREP. It does
not translate those nodes to fabricate an origin fixture. The analytical
reference uses the explicit lower CAD world planes: conduction is
`T=Tleft+(Tright-Tleft)*(x-XMIN)/Lx`; free expansion is
`u=alpha*dT*(x-origin)`. The original matrix translation stays in millimetres,
mesh coordinates/displacements stay in metres, and the receipt binds exact
BREP/request/mesh/raw-field bytes and world bounds/origin. Static solver step
parameters remain unmapped to physical time.

The existing standalone static fixtures retain their default zero origin and
serialized analytical-reference wording. Unit tests check nonzero translated
temperature/displacement fields, unchanged raw world nodes, and rejection of
wrong origins, changed mesh/reference policy, geometry, provenance or weakened
acceptance.

`scripts/verify_fem_imported.py` requires the complete native CAD correspondence
report and re-verifies its portable source bundles. It runs both formulations
at resolutions 2/4/8 for origin and translated solids. It independently checks
mesh/world correspondence and rechecks all native nodal/integration-point fields
against the prescribed reference. Eight pre-output rejections cover changed
geometry/refinement/source bytes, missing provenance, unsupported backends,
weakened numerical acceptance and contact injection. `fem-imported-1` passed all
12 solves and eight rejections using build 21, with maximum normalized field
error `4.685717e-12` under the unchanged `1e-6` gate. Its independently captured
aggregate campaign RAM peak was `142311424` bytes under two CPUs, 2 GiB,
no swap and 128 tasks. [Exact evidence](evidence/imported-cad-fem-cpu.json)
binds the package, CAD prerequisite, raw report and separate failure provenance.
This initial allowlist supports controlled synthetic axis-aligned boxes; it
does not qualify contact, preload, seals, production materials or imported
whole-device environmental conclusions.

## Durable imported references

`harbor-cad --socket SOCKET fem-imported REQUEST.json` and simulation-profile
MCP `fem_plan_imported` return a version-8 plan and approval digest. Requests
contain `source_job`, `region_name` and `spec`; `spec` contains the independent
FEM `reference`, `material_provenance`, `boundary_provenance` and version 1.
Native geometry comes from the registered source, preserving original world
bounds, BREP bytes and millimetre placement. Reference dimensions, resolution
and geometric tolerance must agree exactly with source policy. Unknown
provenance, non-synthetic source and contact injection reject.

Use the same `job submit`/MCP `job_submit` for execution. Version 8 binds CAD,
material, boundary and numerical identities without the prior fluid/static or
thermal envelopes. Older plans reject injected imported recipes. Durable source
staging, orphan recovery, immutable approval/idempotency, cgroup controls and
closed exports reuse the source-bound worker path. Its dedicated
`runtime-fem-imported-worker` exposes only the Gmsh/CalculiX operation closure
and one retained read-only BREP; ten checks bind isolation. Native receipts bind
mesh/raw field bytes and approved original world origin. Rust checks the native
mesh independently, including every C3D8 Gauss-point orientation and volume,
then enforces the unchanged static numerical gates. The exact packaged worker
CLI/MCP/export/lifecycle gate passed at build 23 as `fem-imported-worker-1`,
after the matching standalone `fem-imported-2` repeated all 12 solves and eight
rejections at the changed sandbox envelope. The maximum normalized native
field error remained `4.685717e-12` under the unchanged `1e-6` gate.
CLI conduction and MCP translated free expansion each exported 29 artifacts;
service-tree RAM peaks were `166412288` and `42340352` bytes under explicit
2-GiB/no-swap/two-CPU/128-task controls.

[Exact worker evidence](evidence/fem-imported-worker-cpu.json) binds package,
runtime, qualifier, native/worker report and source identities. Approval/source
mutations, distinct-inode retention, raw world-origin field checks, worker
restart/idempotency, forced complete-tree death, cancellation and final
reservation/root release all passed. Broader imported geometry, contact and
physical validation retain their separate gates.
