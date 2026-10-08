# Bounded immutable studies

Studies are explicit named collections of at most 16 existing execution plans.
Every plan keeps its original recipe version, complete scientific inputs,
observation schedule, independent approval and ordinary job identity. A study
does not synthesize parameter values, change scientific gates or launch a second
scheduler. The existing worker and shared admission own all case execution.

The version-1 study request contains `name`, `provenance`, `cases` (unique `name`,
complete `plan`, `approved_digest`) and `max_total_artifact_bytes`. The latter
must cover the sum of all approved per-case output allowances. Total serialized
intent is bounded by the existing protocol message limit. Each case must already
be a valid plan under the worker's effective policy; duplicate plan identities
and source-bound recipes requiring independent retained-input staging reject.
The initial study slice supports independent v1 CPU analytical, v5 synthetic FEM,
v6 thermal/snow, v9 wetting, v10 contact, v11 thermal/contact, v12 freezing,
v13 spectral and v14 atmospheric plans. Imported CAD/source-transfer operations
remain independently submitted through their existing staging-aware entrypoints.

`study submit REQUEST.json --idempotency-key KEY` sends one collection to the
same worker. All scientific approvals, execution profiles, runtime files and
authority requirements are checked before any study intent or child job is
written. A durable SQLite intent binds the complete collection, original profile
and runner/runtime identities before child submission. Stable child keys derive
from that intent's UUID and case identity. A crash between children leaves an
explicit partially submitted intent; repeating the same key resumes existing
jobs and the remaining submissions without duplicating or upgrading them.
Changed collections, profiles or execution bindings reject. Failed/cancelled
children keep their terminal identity on retry.
The test-only foreground policy keeps its existing weaker restart guarantees;
an interrupted child remains interrupted and is reported as a completed failure.

Acknowledgment requires every child submission to be durable. Resource admission
may leave child jobs queued; acceptance of a collection does not reserve all
resources or imply any solve succeeded. `study status STUDY_ID` returns the
original names, plan/science identities and current ordinary job states. It
distinguishes an incomplete submission, pending execution, completed successful
execution and completed execution with failures/cancellations. Every child uses
the normal logs, cancellation, qualification and checksummed export commands.
Numerical verification, convergence and physical validation remain per-case
evidence and are not inferred from collection completion.

Simulation/all MCP `study_submit` and `study_status` use these same operations.
The collection contract and recovery tests exercise real SQLite state and the
real worker. Native study execution and scalar comparison qualification are
separate measured campaign gates.
