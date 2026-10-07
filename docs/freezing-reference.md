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

Selected Stefan numbers are 0.05–0.2. Resolutions 32–128 are multiples of eight,
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

The thin OpenLB diagnostic builds within a 2 GiB compile scope. Its native
energy/front campaign is queued through the normal host lease. Exact-package,
worker and source-bound retained-water-transfer qualification remain pending.
Neither pressure, volume expansion, fracture nor freeze–thaw lifetime follows
from this reference.
