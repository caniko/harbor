# Cold-restart input inspection

`case validate-cold-restart SPEC.json` and MCP `cold_restart_validate` in
`simulation`/`all` inspect the same Rust-owned version-1 `ColdRestartSpec`.
The CLI runs offline; MCP uses the existing bounded worker protocol. Neither
command opens CAD, creates a job or launches a thermal solver.

The specification binds a geometry digest, named thermal region, explicit
material properties and a declared absolute-temperature domain. Density,
temperature-dependent conductivity and heat capacity, operating histories and
the convection coefficient each have a required availability record:

- `known`: data, nonempty provenance and an explicit synthetic flag;
- `missing`: a nonempty reason, with no replacement value.

Known property curves use ordered piecewise-linear knots with explicit units.
They must cover the entire declared domain, and interpolation outside their
range is rejected. A future native solve must stop if its state leaves this
domain. Absolute Celsius and Kelvin are distinct from temperature intervals.
Known zero convection is an explicitly insulated convective boundary; it does
not eliminate other heat paths. Velocity cannot be supplied as a coefficient.

Ambient and heater histories cover time zero through the exact approved
duration, with explicit piecewise-linear interpolation. Duplicate or unordered
times, gaps at the interval endpoints, wrong dimensions and extrapolation fail.
Prescribed heater energy is the trapezoidal integral of that declared power
history, in joules. This is an input-energy calculation, not a temperature
solution or an assertion that all the energy remains in the component.

The report preserves missing fields, labels synthetic properties/histories even
when the geometry itself was not declared synthetic, and separates input
validity from execution, numerical verification, convergence and physical
validation. `inputs_complete` includes the moisture assessment. Every report
contains one of:

- missing moisture inputs;
- justified inapplicability;
- supported dew-point screening;
- explicitly unsupported screening.

The existing Magnus-over-water air model is bounded to 0–50 °C and valid
relative humidity. Subzero surface inputs report an unsupported ice/frost
assessment rather than applying a water dew-point result as a freezing model.
Supported screening reports the surface/dew-point margin and limitations;
it does not estimate condensate mass or moisture transport.

Native region verification, transient thermal execution, interface/contact
models and prototype evidence remain independent acceptance work. This
inspection makes missing physical input and numerical policy explicit before
that work; it does not infer electronics boot reliability or seal performance.
