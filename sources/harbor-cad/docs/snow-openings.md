# Prescribed snow and planar openings

`case validate-snow-openings REQUEST.json` and simulation/all MCP
`snow_openings_validate` evaluate explicit named planar rectangular openings
against at most 32 prescribed, axis-aligned closed snow prisms in the same world
coordinate frame. Input coordinates and geometric tolerance carry length units;
original units, names and provenance remain bound in the preparation digest.
Each opening explicitly selects its normal axis and plane coordinate. Rectangle
coordinates follow the other two axes in increasing world-axis order.

`examples/snow-openings.json` supplies two overlapping prescribed snow prisms
over a 40-by-40-mm vent. The independent union is 1,100 mm², leaving 500 mm² of
geometrically open area; simply adding prism intersections would double-count
100 mm². Rust tests independently count integer unit-square coverage over 64
overlapping/clipped arrangements, exercise every normal axis and preserve small
remaining slits. A real CLI/MCP test checks units, original inputs and typed
refusals through the same worker.

Intersecting a closed prism with the opening plane gives one clipped rectangle.
The result measures the union of those rectangles, accounting for overlap exactly
once. It reports original aperture area, covered area, remaining area and the
covered fraction for every opening. Prism tangency to the plane is included;
zero-area edge contact contributes no covered area. Coordinates are neither
rounded nor expanded by the tolerance. Geometry narrower than its declared
tolerance or unresolved by original Float64 coordinates is refused.

This is a prescribed geometric blockage calculation. It executes no solver and
does not infer snow deposition, adhesion, permeability, air/water throughput,
thermal coefficients or sealing performance. The independent prescribed-snow
thermal history uses its own approval and material/time-scale gates; area loss
cannot be silently converted to convection. Physical validation stays
unqualified. Arbitrary CAD openings and non-axis-aligned snow remain outside this
first bounded geometry contract.
