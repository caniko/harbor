# Packaged artifact acceptance

`lib.packageTests.mkCheckedArtifact` makes a consumer's acceptance command a
dependency of its deployable output. It snapshots the complete artifact tree and
any related packages, runs acceptance, validates the report, and copies the
tested artifact into an output containing `.harbor/acceptance.json`.

```nix
packageTests.mkCheckedArtifact {
  inherit pkgs;
  artifact = assembledSite;
  artifacts.backend = registry;
  requiredTests = ["desktop::review-progress" "mobile::review-progress"];
  nativeBuildInputs = [pkgs.nodejs pkgs.chromium];
  command = ''
    node ${acceptanceRunner}
  '';
}
```

The runner receives `HARBOR_ARTIFACT`, `HARBOR_ARTIFACT_MANIFEST` (the pre-test
SHA-256 inventory), and `HARBOR_ACCEPTANCE_REPORT` (the required output path).
Serve the supplied artifact with the supplied backend and disposable data. Do
not rebuild application assets inside acceptance. Verify HTTP response bytes
against the inventory, including the JS/WASM the browser actually executes.

Reports use `{"tests":[{"id":"desktop::review-progress","status":"passed"}]}`.
Every required ID must be present exactly once. Empty runs, duplicate IDs,
missing tests, skipped or failed tests are rejected. Runners must normalize
unexpected passes and flaky retries as failures; Bekiper's Playwright adapter
requires one passing attempt with an expected outcome. Artifact inventories are
checked again after acceptance and after copying. `.harbor/` is reserved for
acceptance metadata and excluded from the file inventory.

`lib.packageTests.mkArtifactVerifier { inherit pkgs; }` produces
`harbor-verify-artifact` for service/deployment verification:

The implementation is the `harbor-evidence` Rust library and packaged binaries.
The composable frontends expose `harbor dev artifact snapshot`,
`harbor dev artifact accept`, `harbor prod artifact verify-local`, and
`harbor prod artifact verify-http`. The compatibility entry point below retains
the existing argument contract. Schema-one manifests from previous accepted
outputs remain valid. Manifest writes are private, atomic and synced; snapshot
refuses an existing destination, and acceptance replaces it only after the report
and original artifact bytes are verified. HTTP checks stream hashes under a
15-second per-asset deadline and refuse redirects. No Python runtime is required.

`harbor_evidence::execution::record_step` journals a caller-supplied executor
exactly once. It binds the executable and source/input files before and after
execution, syncs and hashes private raw stdout/stderr, and writes an atomic
receipt before launch. Pre-spawn refusal, normal failure, timeout, cancellation,
unknown effect and source drift retain distinct statuses. A `MayMutate` step
requires application reconciliation even after exit zero; a process receipt
does not certify application acceptance. Interrupted journals and partial logs
remain available for reconciliation. Existing receipts refuse replay.

The executor callback owns the configured command and must execute the supplied
identity, preserve its logs, and settle its process group before returning a
terminal result. This library does not interpret arguments or certify that an
application's effects have been reconciled.

### Rust cutover qualification

The 12 October 2026 cutover passes 13 native tests, strict all-target Clippy and
Cargo package verification. Original helper/test bytes are checksum-bound under
`/data/nvme0/can/ProjectState/evidence/syndb-rust-ops-migration-20261012-NX0v9tLv/harbor-original/`.
The earlier artifact package/adapter acceptance passes. Final package/adapter
qualification for the execution-journal additions encountered bounded evaluation
lease refusals, followed by a denied native-evaluation shell invocation. It
remains open, as do upstream publication, released consumer pins and the denied
formatter-identity capture.

```sh
harbor-verify-artifact verify-local --manifest SITE/.harbor/acceptance.json \
  --root SITE --backend BACKEND_STORE_PATH
harbor-verify-artifact verify-http --manifest SITE/.harbor/acceptance.json \
  --url https://example.invalid --prefix app/ --attempts 1
```

Local verification rejects missing proof, altered site files and a backend store
path different from the tested package. HTTP verification hashes the landing
page and all files under the selected app prefix, rejects redirects and checks
against the expected local manifest. It is read-only and does not require an
authenticated dataset. Use it against both the service listener and the public
origin when checking reverse-proxy routing.

The consumer owns the behavioral contract and test IDs. These checks prove that
the selected artifact passed that contract; they cannot ensure an operator
activated the newest source revision. Deployment must select this checked output
and compare responses with its manifest. An intentionally selected older checked
closure remains a valid rollback.
