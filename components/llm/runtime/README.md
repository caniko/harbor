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

`dispatch_initial` connects this fence to one caller-supplied authenticated
exchange for a v2 create, submit or execute command. It checks the original
registration/binding and target, then reserves UNKNOWN before calling the
exchange once. Only a matching observed receipt can be retained. Missing, lost,
rejected or mismatched replies never permit a repeat. Handles equal the
preallocated execution ID; submission and release bind the same canonical
`(handle, inbox ID, input-byte digest)` tuple. Request IDs are correlations only.
This helper does not reconnect, launch or supervise a bridge. The exchange must
enforce its deadline and private executable/channel custody; current-authority
cancellation, revocation and closure are deliberately unsupported on this path.

`ipc::Exchange` owns the supervisor-supplied private Unix socket and authenticated
command channel. Required limits cover whole-channel elapsed time, records and
wire bytes in both directions. Each partial read/write uses the remaining
deadline; slow peer progress never resets it. Length checks and cumulative
budgets precede allocation or the next write. Effects use exactly one matched
reply exchange inside `dispatch_initial`; transfers have no response. Any
failure closes the original socket and zeroizes channel keys, without connecting
or retrying. The launcher must provide original peer/executable and descriptor
custody and dispose of its initial key buffers; an owned socket is not a process
container or exclusive engine-writer boundary.

### Linux artifact snapshots

`artifact::Snapshot::capture` copies a trusted digest-selected regular file into
an anonymous Linux memfd, seals writes/size/seal changes and executable bits,
then independently hashes the sealed bytes. Its close-on-exec descriptor starts
at byte zero and stays pinned while the caller retains the snapshot. Source
replacement, deletion or in-place writes cannot change the captured image.
The supervisor must launch from this descriptor, rather than reopen the original
path after hashing. Linux 6.3+ execution seals are required; there is no mutable
file fallback. Byte and elapsed-time limits are required, with a 512 MiB artifact
ceiling and the shared one-hour time ceiling. Capture performs no execution.

The custodian must run file capture inside its killable setup phase because a
cooperative deadline cannot interrupt a kernel-blocked filesystem read. It must
retain the complete immutable dependency closure, independently qualified source
identity and durable evidence. Sealing one image does not pin shared libraries or
JavaScript imports, establish private session writers, authenticate a process or
provide admission or a sandbox. macOS keeps its separate existing managed-host
snapshot boundary until a qualified implementation is integrated.

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

The explicit initial-dispatch conformance example accepts the absolute path to a
JavaScript dispatcher module, including one in a realized production package:

```sh
cargo run --locked -p harbor-llm-runtime --example initial_dispatch -- /absolute/path/to/src/execution/dispatcher.mjs
```

It uses the runtime's `ipc::Exchange`, fresh private IPC credentials, the actual dispatcher and pinned generated
client, a synthetic HTTP observer, and disposable controller stores. The observer
checks the on-disk UNKNOWN reservation before each mutation. Normal release, lost
HTTP/IPC replies and bridge exit all exercise no-replay recovery. This is local
cross-language conformance, not the final supervisor, native engine recovery or
privileged custody acceptance.

Rust registry publication depends on first publishing the qualified
`harbor-llm-protocol` release. Both crates are developed in this Cargo workspace;
downstream consumers must use verified registry releases.
