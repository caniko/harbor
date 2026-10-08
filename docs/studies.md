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

`scripts/verify_study_worker.py` is the opt-in exact-package thermal study gate.
It first reconstructs every original standalone thermal case and the separate
temporal/spatial refinements under the unchanged gates. It then submits two
fixed-geometry/history/material cases with explicit native step refinement,
rechecks complete original fields, compares exact retained node values through
CLI/MCP, records per-job resource evidence, and exercises restart, queued-child
cancellation and owned-service death without duplicating the collection. Every
successful or failed child retains its independently checksummed bundle. This
campaign records measurements and does not infer an optimization winner.

The exact packaged gate passed at `c8-2/study-worker` on 2026-10-08, including
both original native field sets, CLI/MCP exact-time comparison, same-invocation
restart and terminal-child recovery. Kernel CPU/memory/task observations and
every report identity are recorded in
[repaired worker evidence](evidence/repaired-workers-20261008.json).

[Equal-accuracy CPU measurements](equal-accuracy-cpu.md) hold the scientific
case and originals fixed while comparing effective one/two-core profiles in
paired repetitions. They are independent of the study's step-refinement
comparison, which changes native numerical accuracy.

```sh
python scripts/verify_study_worker.py \
  --executable /nix/store/CLI/bin/harbor-cad \
  --mcp /nix/store/MCP/bin/harbor-cad-mcp \
  --runtime /nix/store/THERMAL-WORKER.json \
  --authority /absolute/installed-authority.json \
  --native-reference /absolute/exact-thermal-native-gate \
  --output /absolute/fresh-short-path-study-gate
```
