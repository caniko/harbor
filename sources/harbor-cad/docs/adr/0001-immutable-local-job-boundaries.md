# 0001 — Immutable local job boundaries

Status: accepted for the local worker architecture; native qualification is
tracked independently in [qualification.md](../qualification.md).

## Context

CLI and MCP clients can disconnect while a native job is executing. Host
configuration can also change while jobs wait for resources. Neither event
should duplicate a solve, change its scientific inputs, or make an old service
identity authorize a different process tree.

## Decision

Use one Rust worker and SQLite state per root, serving a private Unix socket.
Atomically persist the approved execution-plan digest, idempotency key and
effective host-profile digest before launching. A repeated key returns its
original job only when both immutable bindings match. Native adapters receive
an SI-normalized view; the original quantities remain in the approved plan.

Qualified native jobs belong to tracked systemd user services. Persist the
unit and observed InvocationID, and require a matching live identity before
cancellation. Worker restart reconciles these services. Foreground execution
is a separate, explicitly weaker CPU-reference mode for CI.

Default to one admitted plan per state root. Reserve its RAM/output estimates
and native-copy staging before execution, count retained artifact bytes, and
wait rather than reducing numerical parameters. Hold physical-card locks in
the job process using shared per-user anchors, so compute/render/media roles
and independent worker roots cannot create separate locks for one card.

Copy closed native outputs into verified records without hardlinking mutable
run files. Export a checksum-verified whole bundle with an atomic no-clobber
directory rename. Protocol artifact listings are byte-bounded cursor pages;
they never determine how many records a whole-bundle export includes.

Keep the SI-normalized adapter plan outside the writable native work tree,
mount it read-only, and reject packaged-path traversal out of the store.
Retain failed-attempt snapshots with explicit failed provenance and report
unsafe/over-budget omissions; terminal exports carry their execution state.

## Consequences

MCP exposes typed operations and durable job IDs; it does not contain a second
scheduler or expose shell/Python evaluation. A mutable profile path cannot
change an already submitted job. Legacy jobs without a stored profile cannot
be relaunched under guessed settings.

Execution success, numerical verification, convergence and physical validation
remain distinct. Exclusive card locks establish ownership among Harbor-CAD
jobs; they do not enforce a universal VRAM quota or account for unrelated
display workloads. Package ABI, sandbox, GPU and cross-root aggregate-resource
qualification still require their measured gates.
