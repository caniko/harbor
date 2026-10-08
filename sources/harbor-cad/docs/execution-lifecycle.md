# Job execution identity and retention

## Strict request envelopes

The worker validates the bounded protocol envelope before decoding the typed
operation. A nested schema rejection retains the valid request ID and returns
`invalid_input`, so CLI/MCP clients can correlate it without losing the actual
reason. Raw operation JSON preserves duplicate-field detection. Empty
`doctor`/`backend_list` operations reject extra fields as well. Ambiguous or
malformed outer envelopes and invalid IDs never dispatch an operation.

## Version compatibility

The existing `CaseSpec`, `ExecutionPlan`, and `HostExecutionProfile` version-1
schemas and serialization are unchanged. Their recorded JSON and approval
digests are preserved. `ExecutionBinding` is a separate version-1 record in
`job_executions`, committed with the job and profile. It binds:

- The original plan and effective host-profile digests.
- The canonical runner path, byte size and SHA-256.
- The canonical native runtime manifest and selected adapter/Bubblewrap files.
- Worker protocol version 1 and the explicit native sandbox-policy revision.

Systemd execution requires a packaged runner in `/nix/store`. The weaker CI
foreground path can bind a development executable; changing that executable
rejects execution. The executing process verifies its own identity before
running stages. Queue launch uses the original runner, including after a worker
upgrade. Duplicate submissions with the same plan/profile reuse the first job's
binding rather than upgrade it.

Historical jobs without a binding remain readable and terminal jobs remain
exportable. Their exports explicitly contain `execution_binding: null`; a
missing identity is never guessed from the current worker. They cannot be
automatically relaunched. Unknown execution-binding or sandbox-policy versions
receive an explicit compatibility rejection. Future approval-schema changes
must introduce an explicit version decoder/migration preserving the original
records; adding required or defaulted fields to the existing types is not a
compatible migration.

`execution-binding.json` is a checksummed job artifact. `execution.json` also
includes the durable binding so early failures can still be diagnosed.

## Durable closure retention

Systemd submission registers indirect Nix GC roots for the bound runner, runtime
manifest, Bubblewrap and selected adapter store objects. Nix's reference graph
retains their transitive closures. The job can only be acknowledged or launched
after all roots are registered, their targets checked and `ready.json` synced.
Root management accepts already-realized non-derivation paths with builds and
substitution disabled. Package realization remains an ordinary Canix operation.
Foreground CI jobs record an empty root set and retain their weaker guarantees.

The order is **durable filesystem intent → registered roots → durable ready
record → SQLite job/profile/binding commit → acknowledgement**. A SQLite writer
transaction covers registration and commit. Lost acknowledgements reuse the
existing job; they do not register another root set. The worker's cleanup holds
the same writer lock, so an uncommitted submission cannot be mistaken for an
orphan while its registration is in progress.

Worker restart removes uncommitted orphan intents and roots. Queued and active
jobs keep their roots. Queued cancellation is recoverable. Terminal status alone
does not release a systemd runtime: the unit must be absent or inactive/failed
with matching invocation, and any live or recorded cgroup must be absent or have
`populated 0` (including descendants). Ambiguous ownership retains roots. Cleanup
unlinks only that job's verified root links and metadata, syncing deletion and
keeping the intent until the links are gone. It never deletes store objects or
touches another job's links. The durable database binding remains exportable.
The local full-plan admission reservation remains held while a terminal job's
runtime retention exists, closing the exit-record/service-teardown handoff gap.
The next job stays queued until verified retention cleanup completes.

Unit tests substitute only the GC-registration boundary to exercise partial
registration, lost registration acknowledgement, orphan recovery, committed
submission, duplicate reuse, restart, queued cancellation and delayed tree
termination. Packaged qualifiers separately verify the actual Nix/systemd path.

## Operation-specific importer policy

New bindings use native policy revision `harbor-cad-native-v2`. The importer
policy is `harbor-cad-importer-v1`: Nix `closureInfo` declares the exact FreeCAD
adapter closure, and the worker mounts those individual store objects read-only.
The importer has no broad `/nix/store` bind. Its immutable closure descriptor is
also execution-bound and GC-rooted. Older native runtime manifests lacking that
descriptor receive a compatibility rejection for CAD operations. Historical
bindings remain decodable/exportable; the current worker explicitly rejects
superseded native-policy revisions for relaunch.

Before opening a document, the fixed adapter verifies its actual mounts,
read-only plan/input descriptors, zero effective capabilities, no-new-privileges,
separate network namespace, isolated loopback, absent GPU nodes/session runtime,
and bounded file-size limit. The worker also requires the matching policy in the
fresh FreeCAD success receipt. The checksummed `import-isolation.json` retains
the observed enforcement. The source snapshot stays outside writable outputs.

`scripts/verify_importer_policy.py` launches this path under a **production**
profile with live synthetic credential, Unix-session and host-loopback canaries.
Its two inspection jobs verify valid import and missing-region rejection,
effective service resource properties, immutable input bytes, closure retention,
and complete detached-descendant termination. The fixed diagnostic probes are
enabled by operator environment only; neither documents nor MCP can select
Python code, commands or probe operations. They read no real credentials.

Solver, GPU/JIT and visualization qualification remains independently scoped.
The source advisory review is recorded in `evidence/freecad-security.json`;
runtime isolation does not substitute for patched importer sources.
