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
