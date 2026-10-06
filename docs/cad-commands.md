# CAD inspection and registered regions

`harbor-cad cad inspect case.json` prepares the same immutable plan as
`case plan-cad-inspection`. The case declares unique named solid regions, the
relative source path, original CAD SHA-256, explicit units and chord tolerance.
Planning opens no CAD document. Save the returned plan and submit it with
`job submit plan.json --approve <approval-digest> --idempotency-key <key>` through
the worker. The patched FreeCAD importer sees a private verified read-only copy
of the source inside its existing import sandbox.

After the job succeeds:

```text
harbor-cad --socket <worker.sock> cad regions <job-id>
harbor-cad cad export --state <state-directory> <job-id> <new-destination>
```

`cad regions` returns the schema-versioned `RegionReport`: checked region names,
finite positive solid volumes and bounds in SI, original FreeCAD placement
matrices with millimetre translations, original STL scale, triangles and the
approved geometry tolerance. It reads only checksummed registered JSON from a
succeeded bound native CAD stage. Missing, changed, duplicate, unrelated or
unsupported metadata rejects; a foreign receipt cannot turn a solver/reference
job into a CAD job. The report binds the original science, execution, runner
binding and metadata artifact hashes. It opens no document and launches no
native process. Solid names do not establish ordinal face identities, assembly
correspondence or qualified FEM meshes.

`cad export` first verifies the region report, then performs the same atomic
full-job export/checksum procedure as `artifact export`. It retains the approved
source copy and scientific/provenance records in the portable job bundle.

The MCP `cad` and `all` profiles expose `cad_plan_inspection`, `cad_submit`,
`cad_regions`, `job_status` and `job_logs`. `cad_submit` accepts only the exact
version-1 CAD-inspection/bundle DAG and returns the durable job ID. The results
profile also exposes the read-only `cad_regions` view. Physical validation
remains explicit and unqualified.
