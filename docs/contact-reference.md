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

`case validate-contact-reference REQUEST.json` validates the same strict Rust
schema and reports both analytical states with `executed: false`. Explicit SI
and provenance inputs, unsupported-physics and tolerance rejections match the
native adapter; validation alone never establishes a native solve.

Native attempt `contact-native-1` retains its rejected deck and solver log.
The pinned source's `expansions.f` and `boundarys.f` read numeric fields with
`f20.0`: unrestricted `.17g` exponent strings can exceed 20 characters and
truncate before the exponent completes. The repaired contact serializer keeps
the most significant digits that fit, with maximum relative serialization error
`5e-13`. Exact original approved SI inputs remain in the receipt and all native
field/force/gap checks still compare to those inputs with the unchanged gate.

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
same `0.002` gate. Exact native `contact-native-2` passes against build 37:
fifteen native solves, eight pre-output rejections and five independent spatial
checks. Maximum normalized field/force/gap error is `0.0005404000000002998`;
maximum pressure spread across n2/4/8 is `1.5085714270493817e-7`.
See [exact native evidence](evidence/contact-native-cpu.json).

The first exact packaged worker attempt, `contact-worker-1`, solves n8 native
contact successfully but rejects the fields at the Rust DAT/JSON comparison:
the native bridge retains solver parameters as Float64 (`1.0`, `2.0`), whereas
the Rust parser emitted JSON integers. The corrected parser preserves native
Float64 parameters and validates their numeric values without inventing physical
time. Rechecking the original failed job's complete retained mesh/DAT/JSON/receipt
passes the independent `0.002` field/force/gap gate with maximum normalized error
`0.00017108571428569122`. This diagnostic does not supersede the preserved failed
worker attempt; exact rebuilt worker/lifecycle qualification remains required.

Version-10 `case plan-contact-reference` and simulation-profile MCP
`case_plan_contact_reference` bind the independent SI recipe to a contact→bundle
DAG. Approved submission uses the same durable worker, authority-backed shared
2 GiB RAM reservation, closed exact runtime identities and systemd service tree.
`runtime-contact-worker` contains only this fixed adapter/operation closure.
Plans retain the complete two-state outputs, not invented physical times;
versions 1–9 reject contact injection, including null fields.

Rust independently reconstructs every original DAT node/integration-point value,
matches the complete retained JSON exactly, verifies both translated native
blocks, all affine positive C3D8 maps and complete semantic interfaces, and
recomputes force balance, stress, displacement and opening gates before success.
Historical job qualification also verifies the registered original bytes.
CLI/MCP native execution/export/restart/death/cancellation/release qualification
remains pending at the changed packaged worker identity.

## One-way thermal coupling contract

`ThermalContactSpec` defines two independent prescribed native thermal histories
and mechanical-only contact inputs. Final contact temperatures must be derived
from the complete original C3D8 fields at an exactly retained coupling time.
Both thermal boxes must match the contact dimensions and geometry tolerance,
start at the stress-free mechanical reference temperature, and remain within the
small-strain material range. Each projection has an explicit pointwise loss
limit of at most 1 K and a positive conservation tolerance of at most `1e-10`.
The combined moisture branch is explicit and uses source surface temperatures.

`case plan-thermal-contact examples/thermal-contact.json` and the simulation/all
MCP tool `case_plan_thermal_contact` produce a strict version-11 plan. The plan
contains `thermal-lower`, `thermal-upper`, `projection`, `contact` and `bundle`
stages. Submit it through the same `job submit`/`job_submit` explicit approval
surface as other native jobs. `runtime-thermal-contact-worker` combines the
exact thermal and contact adapters with their separately mounted CPU closures.

The 2026-10-08 exact-package coupling attempt failed its original energy gate at
a 2.5-second integration step (relative error `0.027909769575547243 > 0.02`).
The same packaged driver, physical histories, mesh and gates pass separate native
diagnostics at 1.25 seconds (`0.016145056827222688`) and 0.625 seconds
(`0.010119141175997898`). The example now explicitly approves eight integration
substeps, producing the 1.25-second step while preserving the 10-second energy
output intervals and 10/60/120-second retained times. Full worker coupling remains
a separate exact-package gate; the failed attempt is preserved.

The worker independently checks the complete original thermal DAT/JSON schedules,
positive affine C3D8 geometry, capacitance weights, loss and conservation before
launching contact. Its derived contact descriptor and projection report bind the
approved histories, native file hashes and exact physical/native times. All six
native surfaces retain explicit missing/inapplicable/dew-point assessments;
subzero surfaces retain the unsupported frost-screening status. Static contact
parameters remain separate from thermal physical time. Export retains original
thermal/contact fields, meshes, decks and derived receipts. Historical `qualify
--job` reconstructs the same projection from registered originals and rejects
changed source or report bytes.

CPU tests include nonuniform fields with independently calculated capacitance
weights, source replacement, schema/profile rejection and real MCP planning.
Exact native coupling qualification remains pending. Independent thermal
histories and uniform projected contact temperatures define a one-way
approximation; contact pressure does not supply thermal interface conductance
or feedback.
