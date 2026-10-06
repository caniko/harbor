# Synthetic transient cold/heater/convection reference

`thermal-cpu` is an independent fixed native Gmsh 4.15.2/CalculiX 2.23 CPU
reference. It uses the FEM bridge's isolated Python 3.13 ABI and structured
C3D8 box mesh. `runtime-thermal-cpu` declares its immutable executable and
the bubblewrap launcher. `runtime-thermal-worker` adds a closure-only worker
manifest. Exact native and worker qualification are tracked separately below.

## Model and applicability

The initial temperature is spatially uniform. Material density, specific heat
and conductivity are explicitly prescribed positive SI constants within an
approved temperature domain. The two x-normal faces have the same prescribed
constant convection coefficient; transverse faces are adiabatic. Ambient
temperature and uniformly distributed heater power are piecewise-linear
physical-time histories. Their full intervals and explicit provenance are
mandatory. A zero convection coefficient selects an explicitly adiabatic case.
No coefficient is inferred from flow velocity.

The three-dimensional native model has a one-dimensional independent reference:

```text
rho*c*dT/dt = k*d2T/dx2 + P(t)/V
-k*normal_gradient(T) = h*(T - T_ambient(t)) on x=0,Lx
```

For half-length `a=Lx/2`, Robin modes satisfy `mu*tan(mu)=Bi=h*a/k`.
The uniform-function expansion coefficient is
`4*sin(mu)/(2*mu+sin(2*mu))`. The analytical reference evaluates 256 modes
and exact exponential convolution of each piecewise-linear heater and ambient
segment. Positive retained times require a resolved Fourier number; bounded
Biot numbers and scalar inputs reject unsupported regimes. With `h=0`, the
independent reference is exactly `T=T_initial+integral(P dt)/(rho*c*V)`.
Tests check independently tabulated first eigenvalues, boundary identities,
stable convolution at small time intervals and exact heater-energy integrals.

The conservative maximum-principle temperature bound must lie entirely within
the supplied constant-property domain before meshing. Native temperatures are
checked again. Missing moisture inputs remain explicit; supported reference
requests carry either a missing-input reason or justified inapplicability.
This recipe provides no condensate-mass, electronic boot, sealing or contact
conclusion, and does not replace a whole-device coupled model.

## Native API evidence

The exact CalculiX 2.23 manual inspected for the static FEM reference also
defines this native deck:

- `*HEAT TRANSFER` without `STEADY STATE` performs transient heat transfer;
  density and specific heat are supplied. `SOLVER=SPOOLES` is explicit.
- `*DFLUX` label `BF` is power per volume. Its named amplitude is prescribed
  heater power; the reference multiplier is `1/V`.
- `*FILM,AMPLITUDE=AMBIENT` scales the sink temperature independently of the
  fixed coefficient. Semantic planar faces are mapped to the documented C3D8
  local-node face table after meshing, with complete face-count/area checks.
- `*AMPLITUDE,TIME=TOTAL TIME` preserves piecewise-linear physical histories.
- `*TIME POINTS,TIME=TOTAL TIME` and `*NODE PRINT,TIME POINTS=OUTPUT` select
  independent physical output times. The incompatible `DIRECT` procedure
  option is not combined with time-point output.

Primary source: <https://www.dhondt.de/ccx_2.23.doc.tar.bz2>, inspected
`ccx.tex` sections `heattransfer`, `dflux`, `film`, `amplitude`, `timepoints`
and `nodeprint`. Exact source/manual hashes and interpreter/solver ABI are
recorded in [FEM references](fem-references.md). The analytical PDE calculation
is a numerical verification reference, not a new physics execution backend.

## Evidence and resources

The native bridge retains its Gmsh mesh, native deck, `.dat` node temperatures,
`.frd` temperature/heat-flux output, closed SI retained-field descriptors,
physical-time metrics and request/mesh/field hashes. `.frd` heat-flux association
and serialization are separate from `.dat` point-temperature coverage.
Prescribed initial conditions are labelled, rather than reported as solved
snapshots. Energy accounting integrates the actual linear element nodal
temperatures over mesh volumes and the Robin flux over verified face areas.
Trapezoidal time integration of outward flux is explicitly identified and
tested through refinement. Prescribed heater energy is not retained heat.
Solver increments and energy-output intervals are independent: an explicit
`integration_substeps` (1–64) divides the prescribed `max_step_s` interval for
native integration, leaving its energy-output times and physical histories
unchanged. The native receipt records both schedules. This resolves the first
native attempt's insufficient first-order time integration without changing
the temperature or energy acceptance gates; that failed run remains retained.
The next native attempt reproduced a fixed-width CalculiX input-card rejection:
`heattransfers.f` uses `(f20.0)` fields, so a full Float64 `.17g` exponent can
exceed the numeric field width. Time cards now use bounded 14-significant-digit
scientific text within those 20 columns, with exact input descriptors retained.
The following attempt passed the temperature gate, but exposed seven-digit
stock nodal serialization as insufficient for early heater-energy differences.
The thermal package now carries an owned two-line `printoutnode.f` patch:
only NT/TS scalar `.dat` formatting changes from `E13.6` to `E23.15`, retaining
16 significant digits of native `real*8` temperature. Its receipt records the
patch SHA-256, separately from the unchanged upstream source hash. Static FEM
and other output fields continue using their independent original packages.

`scripts/verify_thermal_cpu.py` exercises adiabatic heating and cold/heater/
Robin histories at three refinements, plus separate fixed-mesh temporal and
fixed-time spatial sweeps. It requires unchanged `0.02` transient temperature
and energy gates on every solve, decreasing spatial continuum-reference errors,
decreasing fixed-mesh temporal solution differences, complete times/IDs,
aligned native physical times for each temporal comparison,
and nine pre-output rejection cases. These transient gates are independently
specified; the static FEM `1e-6` gates remain separate.

The precision-patched fourth native run passed all eight temperature/energy
gates, but failed the original temporal check against the continuum reference.
Its fixed-mesh successive temperature differences were `0.001858940129864095`
and `0.0009295318611748371 K` (ratio `1.9998670379244203`): first-order time
convergence with partial cancellation of the remaining spatial-error floor.
The temporal gate now checks those successive fixed-mesh solutions and records
the observed order independently. Continuum errors, exact physical inputs and
both unchanged acceptance gates remain recorded and required for every solve.
Time self-convergence establishes neither absolute accuracy nor convergence to
the continuum on that fixed mesh; the separate analytical/spatial gates do that.

The first worker run of this package rejected its exact-closure check before
solving. An isolated import reproduced CPython creating
`/nix/store/__pycache__/...fem_reference.cpython-313.pyc` in the namespace's
temporary store root; `python -B` removed the unexpected entry with all 53
declared closure entries still present. Dynamic native adapters now disable
bytecode generation before helper imports, retaining the exact closure check.
Regression tests reproduce immutable top-level helper loading and verify that
an invalid request still rejects before output without adding sibling caches.
The failed run and its precise diagnosis remain retained; no mount is broadened.

`thermal-native-5` passed eight solves, nine pre-output rejections and both
separate refinement assessments at its exact recorded adapter. Its maximum
temperature/energy errors were `0.007558324480261552` and
`0.015384615389630334` under the unchanged independent `0.02` gates. The
aggregate service memory peak was `314224640 bytes` with a 2-GiB/no-swap limit.
Exact revisions, report/runtime hashes and failed-attempt provenance are in
[thermal evidence](evidence/thermal-cpu.json). The bytecode-repaired worker
package is a distinct runtime and requires its own native/lifecycle gate.

`thermal-native-6` passed the complete fresh standalone gate for the
bytecode-repaired adapter at `c1a58dc`, with the same recorded numerical errors
and time-convergence order. Its aggregate peak was `226689024 bytes`. Retained
raw observations also passed the stronger aligned-native-time recheck at
`76fdcd4`; that post-check is hash-bound separately. The corresponding CLI/MCP
worker lifecycle/export gate is in progress, with its exact package combination
and interface-source parity recorded independently.

Bounded native time/output counts and explicit mesh refinement are checked
before execution. The opt-in qualifier runs under the guarded Canix runtime
lease and a bounded 2-GiB/no-swap/two-CPU service. Worker integration and its
operation-specific sandbox have separate qualification. Neither standalone
execution nor process exit establishes physical validation or solver resume.

## Immutable worker planning

`harbor-cad case plan-thermal-reference REQUEST.json --policy research` returns
`{approval_digest, plan}`. The generated `ThermalReferenceSpec` schema describes
the strict SI request; there are no inferred physical inputs. An independent
version-6 plan binds the full histories, provenance, material domain, explicit
moisture assessment, spatial resolution, solver substeps, physical observations
and independently approved temperature/energy tolerances. Versions 1–5 retain
their serialized identities and reject injected thermal capabilities.

The DAG is `thermal_reference → bundle`, with CPU-only stages. Admission requires
same-user authority, systemd ownership, an exact approved immutable native
runtime and a conservative 2-GiB RAM reservation; output has an independent
conservative budget. The `harbor-cad-thermal-cpu-v1` sandbox mounts only the
operation's Nix closure plus a read-only exact request descriptor and private
stage output. It verifies all eight CPU isolation boundaries before meshing.

MCP `case_plan_thermal_reference` is available in `simulation`/`all` and uses the
same Rust planner. Ordinary `job_submit` returns a durable job ID after explicit
digest approval. `qualification_report` and CLI `qualify --job JOB` inspect
checksummed historical evidence, retaining physical validation as unqualified.
No thermal history becomes a boot, moisture or contact result.

`scripts/verify_thermal_worker.py` requires matching immutable CLI/MCP/runtime
and a standalone report for the exact adapter. It exercises both interfaces,
changed-history and wrong-digest rejection, independent raw-field/energy
rechecks, full checksummed exports, closure isolation, same-invocation worker
restart/idempotency, forced complete-tree death, cancellation and final
reservation/root release. This new package combination requires its own native
and lifecycle runs; earlier static-FEM qualification is not inherited.
