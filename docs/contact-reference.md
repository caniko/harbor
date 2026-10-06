# Planar contact reference

The independent C reference uses the pinned CalculiX 2.23 CPU/SPOOLES stack and isolated
Gmsh 4.15.2 mesh ABI. It models two separate synthetic axis-aligned C3D8 blocks,
an explicit initial gap, displacement-controlled compression and an explicit
linear pressure/overclosure law. Material and interface inputs remain synthetic
and source-labelled. Original gaps are retained; automatic contact `ADJUST` and
geometry healing are excluded.

`contact-reference-cpu` and `runtime-contact-reference-cpu` expose the fixed
native adapter and closure-only CPU descriptor. `adapters/contact_reference.py`
executes two static states: initial-temperature preload, then prescribed final
compression and separate uniform lower/upper block temperatures. Each block has
explicit constant Young's modulus and expansion coefficient; the formulation
fixes Poisson ratio at zero and constrains transverse motion. Independent native
DAT verification checks every displacement/stress component, complete nodal
reaction forces, force balance and original geometric interface opening.
Original native fields, decks and checksummed mesh/provenance remain retained.
Static solver step parameters 1/2 have no inferred physical times.

## Immutable source evidence

The matching official CalculiX manual is retained in
`/data/scratch/tmp/opencode/harbor-cad-fem-research-20261006/ccx.tex`; its archive
SHA-256 is `0fcaf2de8cec51853c4a68b32f7b5a9f277e48c0fece7b8ed4b45320908679b6`.
The source hash and native ABIs are recorded in [FEM references](fem-references.md).

- `*CONTACT PAIR` requires `INTERACTION` and `TYPE`; face-to-face penalty contact
  is `TYPE=SURFACE TO SURFACE`, with both surfaces defined by element faces.
  `ADJUST` changes initial coordinates, so it is absent from this reference.
- `*SURFACE INTERACTION` contains
  `*SURFACE BEHAVIOR,PRESSURE-OVERCLOSURE=LINEAR`. Face-to-face contact needs the
  explicit positive slope K, in pressure/displacement units. At zero overclosure
  pressure is zero. This is the specified numerical interface law, not a measured
  gasket law or an automatic hard-contact inference.
- Complete surface faces are selected by native coordinates, using the documented
  C3D8 connectivity solely to translate semantic planes into solver face records.

Official source: <https://www.dhondt.de/ccx_2.23.doc.tar.bz2>, `*CONTACT PAIR`,
`*SURFACE BEHAVIOR`, `*SURFACE INTERACTION` and C3D8 sections.

## Feasibility reference

The first diagnostic uses identical 1 mm blocks, zero Poisson ratio, explicitly
constrained transverse displacement, E = 100 MPa, K = 10¹² Pa/m and 1 µm total
compression. For a small-strain planar law,

`p = max(0, compression - initial_gap) / (height_1/E_1 + height_2/E_2 + 1/K)`.

Expected pressure is about 47.619 kPa for zero gap. Original reaction forces,
displacements and stress must independently establish force balance and the
contact opening/overclosure before this can become a qualified capability.
The scratch feasibility diagnostic is explicitly separate from numerical
qualification and durable CLI/MCP integration. It uses a closure-only CPU
namespace, bounded no-swap systemd service and the ordinary guarded Canix lease.

Diagnostic 1 rejects the padded OCC face bounds before solve at the original
`1e-8 m` geometry gate. Diagnostic 2 preserves that gate and uses documented
Gmsh `getBoundary(recursive=True)` and `getValue(0, tag, [])` to verify exact planar
vertices and native surface area. The native C3D8 volume/Jacobian checks and
CalculiX solve complete; original DAT and solver records remain in
`/data/scratch/tmp/opencode/harbor-cad-authority-20261005/contact-diagnostic-2/`.
Its successful process exit is feasibility evidence only.

Subsequent numerical gates require separate open/closed, preload and temperature
cases, complete native coverage, immutable geometry correspondence and spatial
refinement. Constant synthetic elastic properties and this planar interface law
do not establish real cold-material behaviour, residual gasket compression,
sealing, friction, adhesion, whole-device contact or hybrid GPU FEM.

The requested normalized displacement, stress, reaction-force and gap tolerance
must be positive and at most `0.002`. Axial displacement and thermal strain are
bounded to `0.001` of block height/strain because the series-compliance reference
is small strain while native face contact uses `NLGEOM`. These explicit bounds
precede native qualification. `scripts/verify_contact_cpu.py` runs closed,
cooled, opened, nonzero-initial-gap and dissimilar-material cases at n2/4/8 and
eight pre-output rejections. Spatial pressure spread and field checks retain the
same `0.002` gate. Native package realization/qualification and Rust CLI/MCP
integration remain pending; analytical verifier tests do not qualify a solve.
