# Standalone retained-field presentation

`render REQUEST.json` asks the same worker to prepare a source-bound presentation
plan. The response's `data` contains `approval_digest` and `plan`; ordinary
`job submit` submits that approved plan and returns a durable job ID.

```json
{
  "source_job": "UUID-of-a-completed-authorized-solver-job",
  "times_s": [0, 20],
  "presentation": {
    "camera": [0.04, 0.025, 0.035],
    "width": 640,
    "height": 480,
    "field": "velocity",
    "range": [0, 0.0015]
  },
  "render": {
    "role": "render",
    "backend": "egl",
    "pci": "0000:03:00.0",
    "backend_uuid": null
  },
  "media": null
}
```

The source is a registered job in this worker's state root. It must have an
immutable field snapshot, execution binding and source authorization. Every
requested time must be retained. The first supported source is the completed
OpenLB channel graph; presentation of interrupted or unbound historical results
requires separate qualification.

Set `media` to an independently selected `media`/`vaapi` device for
render → video → bundle. A null media selection produces render → bundle.
Changing the camera preserves source science; these plans contain no CAD or
solver stage.

## Approval and execution identities

Standalone presentation uses execution-plan version 2 and policy
`harbor-cad-presentation-v1`. The approved plan binds the source job, source plan,
execution-binding and authorization digests, registered snapshot digest,
scientific artifact identity and copied byte count. The new job receives its own
execution binding and current host/device authorization.

The worker copies verified source records into distinct inodes before
acknowledgment. Shared filesystem accounting protects this staging operation;
RAM/card admission controls launch. Copied records are committed to the new
artifact registry and mounted read-only. A durable staging intent supports
interrupted-submission cleanup: unpublished copies are removed only when the
original authorized bytes remain verifiable; otherwise recovery data is retained.

`source-execution.json` preserves original approvals and runtime identities.
Render/frame/video receipts retain `execution_id` for the source science and
`presentation_execution_id` for the new approved execution. Physical timestamps
and video playback timestamps remain independent.

Version-1 plans retain their serialization and approval identities. The explicit
decoder rejects a source property on version 1, missing/null sources on version 2,
unknown fields and duplicate fields. Generated schemas enforce the same version
boundary, including plans nested in worker requests. The decoder uses Serde's
[documented remote derive](https://serde.rs/remote-derive.html#invoking-the-remote-impl-directly)
to preserve strict typed field decoding.

## MCP

The `results` and `all` profiles expose `render_plan(request_spec)` and
`presentation_submit(plan, approved_digest, idempotency_key)`. The latter accepts
only source-bound version-2 plans. Simulation submissions retain the existing
`simulation`/`all` profile boundary. Status, logs and artifacts remain bounded.

## Qualification

`scripts/verify_presentation.py` forks a completed controlled source fixture into
a private state root, preserving its original records and authorization. It
exercises packaged CLI/MCP image/video presentation, changed cameras, retained-time
subsets, worker restart/idempotency, complete-tree cancellation and source hash
preservation. It requires an explicit compatible authority and packaged runtime;
running it is an opt-in hardware qualification operation.
