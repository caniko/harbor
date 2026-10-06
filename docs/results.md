# Registered static result sampling and comparison

`results sample REQUEST.json` and results-profile MCP `results_sample` use the
same read-only Rust worker. The version-1 request contains `job_id`, `field` and
`locations`. The initial allowlist is registered succeeded v5/v8 static FEM:
`temperature`/`displacement` at explicit `{"association":"node","node_id":7}`,
or `heat_flux`/`stress` at
`{"association":"integration_point","element_id":42,"integration_point":1}`.
Requests contain 1–64 distinct locations and reject arbitrary artifact paths,
unmapped times, coordinate interpolation and unsupported fields/associations.

Sampling binds registered JSON, authoritative CalculiX DAT and mesh bytes, recorded science and
execution identities and passing scoped native numerical evidence. Results
retain units, component counts, explicit native IDs, coordinate unit, source
artifact identity and an absent physical time for static data. Arrays remain in
registered artifacts; the response includes only the selected bounded values.
The requested complete JSON field is independently matched to its final native
DAT section and exact node/integration-point set before any value is returned.
The pinned `serde_json` parser uses `float_roundtrip`: the default best-effort
float parser changed a retained near-zero stress (`-5.293956e-23`) by one bit.
Native comparisons retain exact Float64 equality rather than adding a tolerance.

The packaged `results-3` gate passed on 2026-10-06 with eight fields from the
qualified v5/v8 conduction/free-expansion sources, CLI/MCP equality, exact
native DAT coverage and 40 input/byte/identity rejection cases. The report is
`/data/scratch/tmp/opencode/harbor-cad-authority-20261005/results-3/verification.json`,
SHA-256 `d092b8d4f09b4847db339d9e064a32666b75fc8e0286116c7f2bfec5a07283f9`.
Package revision: `55f18c30f55fc6092c70f39c7330856b9b968969`; qualifier revision:
`284d92d8f66d81c937260d19e7f66ec8b3f636f2`. This gate preserved original source
bytes and database counts; same-job comparisons produced exact zero signed
differences. Broader fields, interpolation and mesh-mismatched comparison remain
separate capabilities.

`results compare REQUEST.json`/MCP `results_compare` accept version 1 with
`left` and `right` sampling requests. They require the same field, locations,
units, components and exact mesh bytes. They report `right-left` differences
and the maximum absolute difference. Comparisons have no implicit registration,
resampling, relative-error denominator, numerical acceptance or physical
validation claim. Different geometry or native node/integration associations
reject; material/science/runtime identities stay explicit on both sides.
