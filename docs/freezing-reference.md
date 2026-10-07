# Conduction solidification reference

`case validate-freezing-reference examples/freezing-reference.json` and
simulation/all MCP `freezing_reference_validate` use the same Rust contract and
explicit SI converter. Validation returns `executed: false`.
`case plan-freezing-reference` and simulation/all MCP
`case_plan_freezing_reference` generate the same immutable version-12 plan.
Submission uses the common `job submit` / `job_submit` path and requires the
authoritative same-user admission database and systemd execution profile.

The selected native formulation follows the immutable public OpenLB source:

- [Stefan example at `145cd548`](https://gitlab.com/openlb/release/-/blob/145cd54810b468f4b6fd3ed86b10644264841578/examples/thermal/stefanMelting2d/stefanMelting2d.cpp).
- [`TotalEnthalpyAdvectionDiffusionBGKdynamics`](https://gitlab.com/openlb/release/-/blob/145cd54810b468f4b6fd3ed86b10644264841578/src/dynamics/advectionDiffusionDynamics.h), including native temperature/enthalpy conversion and phase-dependent relaxation.
- [`TotalEnthalpyPhaseChangeCoupling`](https://gitlab.com/openlb/release/-/blob/145cd54810b468f4b6fd3ed86b10644264841578/src/dynamics/navierStokesAdvectionDiffusionCoupling.h), which supplies native temperature and liquid fraction.

## Explicit applicability

The synthetic conduction-only reference begins with fully liquid material at its
melting temperature. The xmin boundary is colder than melting; xmax is
insulated and y is periodic. Density, heat capacity and conductivity are equal
and constant in solid/liquid, with explicit latent heat and a material domain
covering the imposed temperatures. The out-of-plane extrusion is prescribed.
The contract rejects flow, unequal densities, arbitrary imported wetting states,
superheated initial states, unsupported backends and weakened acceptance gates.

The energy zero is solid material at `Tcold`:
`h = cp * (T - Tcold) + L * liquid_fraction`, in J/kg. Native temperature is
`theta = (T - Tcold) / (Tm - Tcold)`. Native enthalpy is
`h / (cp * (Tm - Tcold))`; its initial value is `1 + 1/Stefan`.
For D2Q5 with fixed relaxation time 1, native diffusivity is 1/6, so
`dt = dx² / (6 * k / (rho * cp))`. Refining the grid by two and the step count
by four preserves every physical property and the final physical time.

Selected Stefan numbers are 0.05–0.2. Resolutions 32–256 are multiples of eight,
the periodic width is one eighth of the longitudinal extent, and the final
step count is between `n²/2` and `n²`. Original-field observations include the
initial state and final state; other observations are within that late-time
reference interval. This bounds the native problem independently of output
cadence. A moisture assessment remains explicit, including missing inputs and
unsupported frost-screening conditions.

## Evidence gates

The prescribed maximum front-position error is 0.02 of the longitudinal length;
the maximum normalized temperature error is 0.02. Independent mass/energy
balance tolerances are at most `1e-10`. A runnable native adapter must measure
these from closed original fields and population-boundary exchange, with an
equal-physical-time decreasing-error refinement sequence. Native execution,
refinement and physical validation are separate states.

## Native implementation and observations

`adapters/openlb_freezing.cpp` implements the bounded reference with OpenLB's
native collision, streaming, temperature boundary and phase-change coupling.
The insulated shell starts with the total-enthalpy equilibrium: latent energy
in the rest population, sensible temperature in the moving populations.
The per-cell PSM relaxation field is set independently of collision parameters.
Both requirements are verified against the pinned dynamics/example source;
failed initial diagnostic attempts remain retained.

The grid contains `n+1` longitudinal points and exactly `n/8` periodic rows at
cell-centered y positions. Boundary nodes have zero mass. Every active interior
node has control volume `dx² * extrusion`; its total active volume is therefore
`(n-1) * (n/8) * dx² * extrusion`. Active control bounds are
`[dx/2, L-dx/2] × [0, L/8] × [0, extrusion]`, explicitly recorded for each
resolution. This nodal-control-volume convention must be retained during
comparison or transfer; it is not the entire nominal box volume.

Closed CSV snapshots retain every native temperature, specific enthalpy, liquid
fraction, material and grid identity at each approved observation. A separate
per-step ledger records the actual population-boundary heat exchange in joules.
The Python adapter independently reconstructs the phase/enthalpy relation,
front position, similarity temperature, active mass and energy balance. It
checks every original field, including intermediate approved times, and rejects
incomplete grids, converter/source drift or a failed unchanged numerical gate.
Native summaries are checked against original fields, not treated as authority.

Verified snapshots also produce canonical Float64 VTK XML point grids and a PVD
collection with explicit physical times, SI coordinates, field units and native
IDs. These derived files are byte-verified against complete originals. Scientific
CSV, heat exchange, native receipt and portable exports have independent hashes.

Development protocol attempts 9 and 10 solved nine Stefan/grid combinations under the
normal bounded host lease. Independent replay passes all four gates for Stefan
0.1 at n64/128/256, Stefan 0.2 at n128/256 and Stefan 0.05 at n256. The other
three runs fail the fixed 0.02 temperature gate and remain retained. The Stefan
0.1 maximum temperature errors are 0.01894585756434697,
0.015456667961909809 and 0.010499081289470857 over both retained nonzero times;
front errors also decrease. Attempt 10's independent complete-field/flux replay
also generated and byte-verified every portable VTK grid/time collection.
Its energy balance errors are at most `5.813795342024023e-14`; mass errors are
zero. Source, development binary, retained originals and report identities are
recorded in [development freezing evidence](evidence/freezing-development-20261007.json).

The independent Rust replay accepts the same six cases with unchanged gates;
maximum gate utilizations are 0.947293, 0.772833 and 0.524954 for Stefan 0.1
n64/128/256. It reconstructs complete original CSV, boundary exchange and
portable VTK/PVD. Its wrapper request identity, moisture entry and sandbox
canaries are fixture scaffolding, explicitly unqualified for runtime/sandbox/
worker execution. The report hash and preserved failed replay attempts are
recorded in the same development evidence file.

Production outputs `freezing-native-cpu`, `freezing-reference-cpu`,
`runtime-freezing-reference-cpu` and `runtime-freezing-worker` are wired through `nix/freezing.nix`, using the
existing pinned OpenLB/Harbor stack. The operation-specific CPU policy mounts
only its exact closure, read-only inputs and bounded scratch. The packaged gate
is `scripts/verify_freezing_cpu.py --runtime RUNTIME --output NEW_DIRECTORY`:
six accepted solves, three unresolved-temperature failures, twelve pre-launch
science rejections, both initial log layouts and decreasing equal-time errors.
This packaged campaign remains pending: the host evaluation guard currently
refuses its required memory headroom. Development compilation and numerical
replay are separate from exact-package/sandbox and worker qualification.

## Durable worker integration

Version 12 contains only the approved freezing descriptor and a serial
`freezing → bundle` DAG. Older approvals reject a freezing field, including null;
the new plan rejects unrelated case/material/transfer capabilities. Refinement
changes the scientific and approval identities. Allocated padded lattices,
independent reconstruction, every retained CSV/VTK and the complete boundary
ledger determine conservative resource minima; understating them rejects before
submission.

The bound runtime exposes only the freezing adapter/closure. The read-only
descriptor is passed into its own isolated CPU sandbox, and the immutable job
binding uses `harbor-cad-freezing-cpu-v1`. Rust reconstructs the same native
phase/enthalpy relations, analytical front/temperature, complete active grid and
boundary energy independently before success. It checks each VTK Float64 value
against the original CSV, SI geometry/units and physical-time collection.
Both formats are registered with explicit times and native point association.
Historical `qualify --job JOB` checks original registered byte identities and
reconstructs the scientific gates; editing both a field and an unregistered
receipt cannot replace registered evidence.

The exact packaged worker campaign is:

```sh
python3 scripts/verify_freezing_worker.py \
  --executable CLI --runtime WORKER_RUNTIME --mcp MCP \
  --authority AUTHORITY --native-reference QUALIFIED_NATIVE_DIRECTORY \
  --output NEW_DIRECTORY
```

It requires the exact complete native campaign, then verifies successful CLI
and real MCP jobs, immutable approvals, same-invocation worker restart,
idempotency, original-field exports, historical mutation rejection, owned
complete-tree forced death/cancellation, kernel resources and final admission/GC
root release. This campaign remains unexecuted pending normal production-build
headroom; implementation and CPU tests do not qualify its packaged execution.
The integration at `77113a5a928aa016a98425eb38d19b2a9135df00` passes the full
CPU gate: all Rust tests, Clippy with warnings denied, locked build, Ruff and
111 Python tests, including real MCP approval/error parity. Treefmt, pinned Simit
CI drift and Nix syntax checks pass. The Python/Rust manufactured-field tests
reject changed source/geometry, incomplete or duplicate nodes, phase/enthalpy
drift, boundary-ledger truncation, weakened gates, changed portable values/units
and substituted checksums.
Source-bound retained-water transfer is still pending. Pressure, volume
expansion, fracture and freeze–thaw lifetime remain outside this reference's
applicability.

## Original-field queries

`results sample-freezing REQUEST.json` and results/all MCP
`results_sample_freezing` return up to 64 exact original `(i,j)` point values at
one approved retained physical time. The typed field is `temperature` (K),
`specific_enthalpy` (J/kg), or `liquid_fraction` (dimensionless). Requests contain
`schema_version: 1`, `job_id`, `field`, `physical_time_s` and distinct ordered
`points: [[i,j], ...]`. Zero time is supported; interpolation and unretained
times are rejected. The request must identify a succeeded execution-bound
native freezing job with complete registered scientific evidence.

Reports retain source CSV/receipt identities, scientific/execution/binding IDs,
native step, physical time, original point association, SI coordinates and units.
Complete fields and conservation gates are rechecked before selecting values;
querying a few good points cannot hide corruption elsewhere in the grid.
`results compare-freezing` / `results_compare_freezing` accept `schema_version: 1`
and two sample requests under `left` and `right`. They require the same grid,
control geometry, field, physical time and ordered points, then report signed
right-minus-left values. Engineering acceptance remains explicitly unassessed.
