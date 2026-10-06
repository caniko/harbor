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

`results compare REQUEST.json`/MCP `results_compare` accept version 1 with
`left` and `right` sampling requests. They require the same field, locations,
units, components and exact mesh bytes. They report `right-left` differences
and the maximum absolute difference. Comparisons have no implicit registration,
resampling, relative-error denominator, numerical acceptance or physical
validation claim. Different geometry or native node/integration associations
reject; material/science/runtime identities stay explicit on both sides.
