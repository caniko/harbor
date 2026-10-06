# Planar wetting reference

The standalone CPU reference uses OpenLB 1.9.0 at
`145cd54810b468f4b6fd3ed86b10644264841578`, with its two D2Q9 lattices,
well-balanced Cahn–Hilliard coupling and interpolated wetting walls from
[`contactAngle2d.cpp`](https://gitlab.com/openlb/release/-/blob/145cd54810b468f4b6fd3ed86b10644264841578/examples/multiComponent/contactAngle2d/contactAngle2d.cpp).
The upstream three-point estimator has an incorrect coordinate assignment in
its vertical interpolation. Harbor retains complete native phase/velocity and
material fields instead, then independently fits the entire `phi=0.5` contour.

## Scientific contract

This reference is explicitly synthetic and two-dimensional, with equal liquid
and vapor density/kinematic viscosity, constant properties, periodic x and
impermeable planar y walls, a prescribed uniform contact angle and no gravity.
SI diameter, diffuse-interface thickness, density, kinematic viscosity and
surface tension are explicit with material and boundary provenance. The fixed
Navier–Stokes relaxation time is 1; the phase relaxation time is explicit.

For `dx = diameter / resolution`, the pinned converter gives
`dt = (tau - 0.5) dx² / (3 nu)`. Surface tension in lattice units is
`sigma dt² / (rho dx³)`. The wall lies at `y=dx/2`. The physical interface
thickness is held fixed across refinements. Native sampling is in SI world
coordinates; raw lattice velocity remains labelled as lattice velocity.

The supported bounds are n24–96, 100–100000 steps, at most 32 observations,
60–120° contact angle, at least three cells across the interface and lattice
surface tension no greater than 0.02. Descriptors require distinct initial and
final observations. Water–air property ratios and additional capability fields
reject before solving.

## Verification and isolation

`runtime-wetting-reference-cpu` supplies the exact adapter and operation closure.
The bridge requires `harbor-cad-wetting-cpu-v1`: closure-only read-only store,
no GPU nodes/sysfs/host home/session bus/worker socket/network access and a
read-only descriptor. The native library owns collision, streaming and wetting
coupling; the bridge only verifies inputs, launch identity and retained outputs.

Independent checks require complete unique Cartesian coordinates, the two
semantic wall planes, a finite bounded phase field and a single isolated
near-circular symmetric droplet. A centered least-squares contour fit produces
`theta = acos((y_wall - y_center)/radius)`; the wall/interface region is excluded
from the fit, and the maximum relative radial residual must be at most 0.05.
Phase area is `sum(1-phi) dx²` over bulk cells, per unit out-of-plane depth.
The maximum relative phase-mass drift must be at most `1e-3`; the absolute
angle error must be at most `5°`. Inputs can tighten these gates.

`scripts/verify_wetting_cpu.py` exercises n24/36/48 at equal physical duration
for 90° and 100° references. Each solve retains its original field bytes and
uses 24000/54000/96000 steps, respectively (0.016 s at the declared SI inputs).
The shorter initial 0.005333 s campaign retained an unsettled 100° droplet;
that failed settling gate does not establish convergence. Each reference
must pass mass and angle gates; late angle change must be at most 0.2°, and
angle error must decrease across mesh refinements. Unsupported inputs must
reject with empty scientific output directories.

Build/native qualification is pending. This static reference supplies part of
the A1 feasibility foundation. Inlet spray momentum/drop sizes, 3D wetting,
seal/pore ingress, vapor flux, evaporation, transient Stefan verification and
physical validation retain their separate acceptance gates.
