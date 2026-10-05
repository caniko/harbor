# Job execution identity and retention

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

Unit tests substitute only the GC-registration boundary to exercise partial
registration, lost registration acknowledgement, orphan recovery, committed
submission, duplicate reuse, restart, queued cancellation and delayed tree
termination. Packaged qualifiers separately verify the actual Nix/systemd path.
