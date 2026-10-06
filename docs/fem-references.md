# Independent Gmsh/CalculiX CPU references

The `fem-cpu` candidate is a fixed native bridge for two explicit synthetic
references. Native Gmsh 4.15.2 creates a structured linear C3D8 box mesh;
CalculiX 2.23 executes the requested CPU solve with `SOLVER=SPOOLES`.
It uses a separate Python 3.13 interpreter and the corresponding Gmsh module.
Discovery and MCP import neither native library. No GPU execution is inferred
from a CPU result.

## Immutable API and source evidence

- Locked Nixpkgs `73e728ddb6b7a12d18808f510813a13ee1fe4cce`,
  `pkgs/by-name/gm/gmsh/package.nix`: Gmsh 4.15.2 archive hash
  `sha256-vj9m8iXSe6n6AU8H6DFpKF2ooFGw6KtxA9iAZrOb3T4=`.
  The official [API documentation](https://gmsh.info/doc/texinfo/gmsh.html)
  currently identifies the same `gmsh_4_15_2` source. Inspected interfaces:
  OpenCASCADE box/boundary/area queries, transfinite constraints, physical
  names, `getNodes`, `getElements`, `getIntegrationPoints`, and `getJacobians`.
- The same Nixpkgs pin's `pkgs/by-name/ca/calculix-ccx/package.nix`:
  CalculiX 2.23, source archive SHA-256
  `9c88385c10fb04f5dc6c4e98027a51bebdd8aee3920e05190d6c1dd08357d6e7`.
  Its existing CPU recipe uses LP64 BLAS/LAPACK/ARPACK and SPOOLES.
- Official [CalculiX sources and manual](https://www.dhondt.de/): downloaded
  `ccx_2.23.src.tar.bz2` and verified that exact source hash. The matching
  manual archive `ccx_2.23.doc.tar.bz2` has observed SHA-256
  `0fcaf2de8cec51853c4a68b32f7b5a9f277e48c0fece7b8ed4b45320908679b6`.
  Inspected `ccx.tex` sections `*HEAT TRANSFER`, `*NODE PRINT`, `*EL PRINT`
  and `*EXPANSION`, and exact `printout.f` field headings.

Source/manual research is retained under
`/data/scratch/tmp/opencode/harbor-cad-fem-research-20261006/`.
The exact standalone reference package at revision
`60defb0f7f239b1d0abb5d9f9615edb12b7579d1` passes the bounded native gate for
both formulations at resolutions 2/4/8 and seven pre-output rejection cases.
The raw report is
`/data/scratch/tmp/opencode/harbor-cad-authority-20261005/fem-native-2/verification.json`;
runtime `/nix/store/35nn40275w6ng653dnsqxib7pss6kp1j-harbor-cad-fem-reference-runtime.json`.
Worker integration has separate qualification below.

## Scientific and geometric contract

An exact version-1 request declares `synthetic: true`, `backend: "cpu"`,
`size_m` (three positive lengths), `resolution` (2..32 cells per axis),
`geometry_tolerance_m`, two `temperatures_k`, and an unchanged maximum
normalized-error `numerical_tolerance` of at most `1e-6`.

- `mode: "thermal_boundary"` adds constant `conductivity_w_m_k`. The x faces
  have fixed temperatures, the remaining faces are adiabatic. Independent
  references are the linear temperature distribution and Fourier heat flux.
- `mode: "free_expansion"` adds `young_modulus_pa`, `poisson_ratio`, and
  constant `expansion_per_k`. Initial/uniform final temperatures and symmetry
  face constraints give `u = alpha * delta_T * x` and zero stress. The input
  must satisfy the small-strain range, `|alpha * delta_T| <= 0.01`.

Geometric predicates select complete planar xmin/xmax/ymin/ymax/zmin/zmax
faces by bounds and area. Numbered faces carry no semantic identity. The bridge
checks one six-face box, complete boundary node sets, positive Gauss-point
Jacobians, integrated volume, and exact node/element result coverage. Mesh
coordinates remain metres and fields preserve Kelvin, W/m², metres and Pa.
Static CalculiX step parameters are retained as solver parameters, with no
invented physical-time mapping. Native `.msh`, `.inp`, `.dat`, `.frd`, JSON
mesh/fields and execution/numerical receipts are retained separately.

`scripts/verify_fem_cpu.py` is the opt-in packaged gate. An independently
bounded no-swap/two-CPU/2-GiB user service runs both formulations at resolutions
2/4/8 in fresh CPU-only namespaces and rejects unsupported backends, modes,
material/temperature inputs, excessive refinement and weakened error gates.
It records effective kernel limits before/after execution and artifact hashes.
The native solver computes in Float64; CCX's `.dat` serialization uses its
documented 7-8-significant-digit text formats. Original text and mesh files stay
available, and numerical comparison must pass at that actual output precision.

These references cover constant properties and synthetic procedural geometry.
Transient heater/convection histories, imported-CAD mesh correspondence,
contact, conservative one-way field exchange and hybrid PaStiX/PaRSEC require
their own execution and evidence. The references establish neither physical
validation nor component boot reliability.

## Independent version-5 worker plans

`case plan-fem-reference SPEC.json --policy research` produces an immutable
version-5 plan and approval digest. `FemReferenceSpec` uses explicit SI field
names and labels synthetic geometry/material values. It has its own science
identity and contains no fluid `CaseSpec`, camera or invented physical-time
observations. Plans v1–v4 retain their original serialization/digests and reject
FEM injection. The simulation MCP profile's `case_plan_fem_reference` returns
the same plan, and `job_submit` returns the same worker's durable job ID.

The worker executes Gmsh/CalculiX in `harbor-cad-fem-cpu-v1`, with the immutable
operation closure only, no GPU/sysfs/session/home/network access, a read-only
request and a stage-local writable directory. The native receipt checks these
boundaries, binds exact request/source identities, preserves positive mesh
Jacobians/volume and complete node/integration-point fields, and enforces all
approved numerical tolerances. `runtime-fem-worker` exposes that exact
operation manifest, independently of `runtime-fem-cpu` used for native gates.
Submission requires installed same-user authority, reserves 2 GiB CPU RAM and
bounded mesh/field/output copies, and retains active runtime closures. The
reservation is an admission estimate; systemd enforces RAM/no-swap/CPU/tasks.

`scripts/verify_fem_worker.py` qualifies exact-package CLI/MCP field integrity,
analytical rechecks, isolation, service peaks, worker restart/idempotency,
forced death, cancellation and admission/root release. Its package gate is
pending. Historical `qualify --job` reports these two static formulations'
checks separately from current-runtime qualification, convergence and physical
validation. Imported CAD, contact, transient thermal history and hybrid GPU
factorization remain separately scoped work.
