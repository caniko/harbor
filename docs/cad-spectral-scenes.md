# Source-bound CAD spectral materials

`cad prepare-spectral-scene REQUEST.json` and MCP
`cad_prepare_spectral_scene` (`cad`, `results`, `all`) verify a registered
source and return a bounded preparation report. The request supplies ordered
explicit wavelength quantities, a geometry tolerance, named optical materials
and one provenance-backed assignment for **every** imported named region.
The CLI and MCP use the same Rust worker; preparation opens no CAD document
and changes no source artifacts or job state.

The initial supported geometry is the existing top-level closed planar box
scope. The report binds each region's original BREP/import evidence,
science/execution/authorization identities and registered binary STL checksum.
Whole-region material assignments cover every facet in its original order;
face numbers and display colours supply no spectral material identity.

## Precision and geometry gates

The pinned FreeCAD [Mesh writer](https://github.com/FreeCAD/FreeCAD/blob/4fd3bf320d9566a27e60069fc8387448aaa3a094/src/Mod/Mesh/App/MeshPyImp.cpp)
maps `STL` to binary STL (`BSTL`). The reader requires its exact complete
50-byte facet records, finite original Float32 positions/normals and zero
attribute bytes. It preserves coordinates and winding, converting millimetres
to Float64 metres once. The placement is already present in world coordinates;
the retained original FreeCAD placement is provenance and is not applied again.

Verification checks nondegenerate outward normals, unique facets, closed
oriented two-facet edges, complete six-plane coverage, world bounds, surface
areas and signed volume. No welding, gap healing or rounding repair is used.
World bounds satisfy the explicit tolerance, which cannot weaken the original
importer tolerance. Area and volume correspondence retain `1e-10` relative
gates. Inputs whose original STL precision cannot satisfy those gates refuse;
the source bytes remain available unchanged. This geometric assessment is
independent of spectral numerical/convergence and physical validation.

## Optical inputs and unknowns

The initial optical formulation is explicitly `opaque_lambertian`, with
`piecewise_linear` reflectance and absorptivity at **every original knot**.
Both arrays must be supplied and lie in `[0, 1]`, closing their sum to one
within `1e-12`. There is no transmission, implicit complement, RGB-to-spectrum
conversion, wavelength extrapolation or guessed reflectance. Each optical
response and dimensionless ageing action carries its own provenance and
synthetic classification through `PhysicalInput`.

Missing optical responses produce `missing_optical_inputs`; missing calibrated
ageing action leaves ageing readiness missing while retaining optical data.
Known inputs produce `prepared_not_executed`. Neither status asserts an
irradiance/dose solve, material lifetime, GPU execution or physical validation.
The scene identity changes with material, association, original geometry or
evidence changes. Triangle arrays stay in their registered source artifacts; the report
contains their identities and aggregate geometric assessments.

[`examples/cad-spectral-scene.json`](../examples/cad-spectral-scene.json) is
an explicit manufactured-material example with missing ageing data. Replace
its source job and tags only with a verified registered CAD job. Arbitrary
assemblies, curved solids, per-face mixtures and transmitting materials remain
outside this geometric/material preparation scope. Native material-tagged
surface transport requires an independently approved solver slice.
