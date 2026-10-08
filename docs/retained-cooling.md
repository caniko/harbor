# Retained-distribution cooling: native formulation contract

This continuation implements the retained-water recipe in
[the implementation specification](implementation-spec.md#5-engineering-recipes-and-boundaries)
after the independently qualified wetting, conservative extrusion and Stefan
reference slices. Its acceptance is distinct from those prerequisites.

## Supported synthetic model

The initial model is stationary fixed-volume, equal-density/equal-heat-capacity/
equal-conductivity conduction. Every original active wetting nodal control volume
retains its water amount `f = 1 - phi`, original position, source identity and
explicit extrusion. The background carries the same sensible thermal properties
and no latent heat; the water contribution carries explicitly supplied latent
heat. Native cell enthalpy is
`cp * (T - Tcold) + f * L * liquid_fraction` in J/kg of the complete control.
Water mass is `rho * volume * f`; it is conserved independently of total
background-plus-water thermal mass. These equal properties are synthetic inputs,
not water/air substitutions or physical qualification.

Require the complete recorded native source and exact retained observation.
Original water fractions must lie in `[0,1]` without clipping or thresholding.
Stationary conduction requires exactly zero original velocity. Moving states
and phase overshoots remain available as retained evidence and refuse this
native initialization. No wetting result supplies temperature: initial liquid
water at an explicitly prescribed melting temperature, cold-boundary temperature,
material domain and provenance are independent approved inputs. Moisture-risk
inputs remain explicit.

The lower y wall is an explicit cold Dirichlet boundary, the upper wall insulated
and x periodic. Walls carry no transferred mass. The original active controls,
including all original x points, define the domain. Native geometry may neither
discard an endpoint nor silently substitute the earlier Stefan strip geometry.

## Native source and thin driver

Use the existing immutable OpenLB `145cd54810b468f4b6fd3ed86b10644264841578`,
Float64 CPU, D2Q5 total-enthalpy collision/streaming and native boundaries.
Its `dynamics::ParameterFromCell` supplies per-control latent heat `f * L` to
`TotalEnthalpyAdvectionDiffusionBGKdynamics`; a narrow parameter-routing coupling
invokes the original `TotalEnthalpyPhaseChangeCoupling` with the same per-cell
latent value. The original solver owns phase/temperature evolution. Retain
upstream source links from [the Stefan reference](freezing-reference.md).

No flow evolution, expansion, pressure, deformation, fracture or lifetime is
inferred. Native phase/temperature, initial state, original source phase/velocity
and per-step population-boundary energy exchange must remain closed authoritative
artifacts with units and independent source/execution identities.

## Acceptance and delivery order

1. Validate explicit thermal inputs and independently reconstruct conservative
   source-to-initial-enthalpy mapping. Refuse overshoots, moving states, missing
   temperatures/provenance and weakened gates.
2. Verify a thin native driver against the uniform-phase Stefan reference and
   nonuniform original retained distributions. Independently reconstruct original
   phase/enthalpy relations, mass and native population-exchange energy at every
   retained time; conservation gates remain at most `1e-10`.
3. Assess equal-physical-time spatial and temporal refinement independently, at
   at-most-`0.02` normalized errors. Any subcell refinement must explicitly copy
   the complete parent phase into congruent subcontrols and conserve parent mass
   and initial enthalpy; it cannot re-evaluate or smooth the original wetting
   solution. A numerical replay cannot qualify native execution.
4. Route the independent immutable cooling approval through the common worker,
   transactionally retain distinct-inode originals before acknowledgment, and
   qualify exact packaged native execution, CLI/MCP parity, results, exports,
   restart/idempotency, cancellation/death and root/reservation release.

Use existing Rust/Serde/Schemars contracts, Rust integration tests, Pytest native
reconstruction tests and operation-only Nix packages. Verify each slice with
`cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`,
`cargo build --locked`, focused `uv run --locked pytest`, and the complete
`python3 -B scripts/check_cpu.py` gate before its local commit. Package/campaign
execution uses the existing guarded Canix lease and immutable source capture.
This document defines acceptance; it records no executed cooling capability.
