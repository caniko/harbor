# Conduction solidification reference

`case validate-freezing-reference examples/freezing-reference.json` and
simulation/all MCP `freezing_reference_validate` use the same Rust contract and
explicit SI converter. Validation returns `executed: false`; there is no
freezing-job submission path yet.

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

Development protocol attempt 9 solved nine Stefan/grid combinations under the
normal bounded host lease. Independent replay passes all four gates for Stefan
0.1 at n64/128/256, Stefan 0.2 at n128/256 and Stefan 0.05 at n256. The other
three runs fail the fixed 0.02 temperature gate and remain retained. The Stefan
0.1 maximum temperature errors are 0.01894585756434697,
0.015456667961909809 and 0.010499081289470857 over both retained nonzero times;
front errors also decrease. Energy balance remains below `1e-10`.

Production outputs `freezing-native-cpu`, `freezing-reference-cpu` and
`runtime-freezing-reference-cpu` are wired through `nix/freezing.nix`, using the
existing pinned OpenLB/Harbor stack. The operation-specific CPU policy mounts
only its exact closure, read-only inputs and bounded scratch. The packaged gate
is `scripts/verify_freezing_cpu.py --runtime RUNTIME --output NEW_DIRECTORY`:
six accepted solves, three unresolved-temperature failures, twelve pre-launch
science rejections, both initial log layouts and decreasing equal-time errors.
This packaged campaign remains pending: the host evaluation guard currently
refuses its required memory headroom. Development compilation and numerical
replay are separate from exact-package/sandbox and worker qualification.

Worker execution and source-bound retained-water transfer remain separate
pending slices. Pressure, volume expansion, fracture and freeze–thaw lifetime
remain outside this reference's applicability.
