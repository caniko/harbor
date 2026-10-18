# harbor-llm-runtime

Controller-side, provider-free execution accounting for Harbor LLM bridges.

The application authenticates original admission and supplies a private,
pre-existing store protected from backend and daemon writers. Register once and
persist the returned registration in the independent application ledger before
dispatch. Never register replacement state to recover a missing ledger/store.

`Controller::reserve` fsyncs UNKNOWN before the caller releases one exact start,
submit, execute or interrupt operation. Submission admits input; execution is a
separate reservation requiring an observed submission and its exact input digest.
A lost reply, process crash or restart never releases that operation again. A
retained observation does not authorize retry. Reopened handles cannot reserve
fresh start, submission or execution work, including when an older journal has
no execution entry. Current-authority interruption remains separately authorized
against the original execution; reopening never grants that authority.
Kernel locks prevent concurrent controller handles; file/directory identities,
private ownership/modes, random lock identity and bounded journal reads detect
missing or replaced state. Reopen only reads existing state. A failed write
poisons the in-memory handle. Interrupted initialization and orphaned output
remain evidence and are not silently removed or recreated.

`retain` accepts complete application-validated output and durably binds its
bytes, digest and execution binding before `deliver` can return identical bytes.
Applications must authenticate the current delivery generation themselves.
Changed bindings and path relocation are not implicitly equivalent. Caller
budgets are required and capped at 16 MiB retained output and 16 KiB journal.

This crate does not authenticate application admission, verify input manifests,
launch a provider, implement a sandbox or stop descendants. Store owner and
parent-directory custody remain the application's trust boundary; same-owner
malicious writers can undo accounting unless a stronger custodian excludes them.
Read-only journal recovery does not prove zero-effect reopening of an actual
OpenCode engine. That requires separately qualified standalone launch behavior.

Run the native fault tests from the Harbor root:

```sh
cargo test --locked -p harbor-llm-runtime --jobs 2
```

Rust registry publication depends on first publishing the qualified
`harbor-llm-protocol` release. Both crates are developed in this Cargo workspace;
downstream consumers must use verified registry releases.
