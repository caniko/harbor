# Application storage contracts

Harbor DB owns database infrastructure, durable backup publication and authority
transitions. Applications retain schema migration, coherent capture, import and
record-level validators. Consumers declare paths, identities and deployment policy.

## Dedicated PostgreSQL provisioning (version 2)

`services.harbor-db.projects.<name>.postgres.provision` declares `database`,
`ownerRole`, `runtimeRole`, `runtimeOsUser`, optional `ownerOsUser` (root), and
schema/current/default privileges. Enable it explicitly. It may be used without
enabling a lifecycle migration. Database and roles must be unique across projects.
Provision project names use 1..128 ASCII letters, digits, underscores or hyphens.
Owner and runtime OS users must differ: one peer identity cannot be authorized as
both the schema owner and the restricted runtime role. `schemaUnits` and
`runtimeUnits` must be disjoint. The combined provisioning order across enabled
projects must also be acyclic, including shared schema and runtime units.
`lib.applicationProvisioning = 2` advertises strict post-schema reconciliation.

Runtime OS users cannot be `root` or `postgres`. Database-scoped peer rules allow
only the declared runtime and owner mappings and the PostgreSQL control account.
Explicit local and TCP reject rules terminate admission before permissive global
HBA rules can authorize an undeclared role.

`schemaUnits` require the generated provision service. `runtimeUnits` require
the generated permissions service, which runs after the explicit schema units.
Harbor DB does not invoke schema migrations itself. `tables` overrides exact
privileges for existing named tables, including append-only tables. New tables
initially receive the declared default privileges; all owner migrations must run
with application writers stopped and reconcile named exceptions before startup.

The control command is `harbor-db-provision --config MANIFEST apply|reconcile|check`.
`apply` retains pre-schema initialization: absent named tables may be created by
the following schema units, while existing tables receive their exact privileges.
`reconcile` requires every declared table and rolls back its privilege transaction
if any is absent. The generated permissions service runs `reconcile` followed by
`check` before admitting runtime units. Custom post-schema callers must also use
`reconcile`. `check` is read-only and returns 2 for missing tables or privilege
drift. The manifest retains
version 1, a policy and an explicit local OS-peer control endpoint. Neither
passwords nor credential contents belong in this manifest. A persistent
cluster-scoped inode serializes apply/reconcile across connection changes.

The current Python regression fixture explicitly creates its declared tables
before testing post-schema admission. Every original assertion and all 174 method
identities remain enforced. Its original setup is retained byte-for-byte under
`tests/oracles/pr14/tests/` and used by the frozen-runtime gate; the current setup
is hash-bound in `tests/runtime-extensions.toml`. Missing declared tables are
separately rejected by both engines and the generated runtime dependency chain.

Existing databases owned by another role, privileged/inherited roles and objects
owned by other roles are rejected. Provisioning does not perform implicit
adoption or role takeover. Explicitly validate and adopt existing storage through
the owning recovery workflow before using this contract.

Roles cannot inherit or be granted to another role. Runtime receives database
CONNECT and schema USAGE, without database CREATE or TEMP privileges. A schema
owned by an unrelated role cannot be taken over implicitly. PostgreSQL's initial
`public` schema owned by `pg_database_owner` is the explicit initialization case.
The runtime role must own no objects in any schema of the application database.
Provisioning rejects such ownership before role or ACL mutation, and read-only
checks inspect PostgreSQL ownership dependencies across schemas and object types,
including tables, sequences, views, functions, types and schemas.
Direct runtime-role ACL dependencies must also belong to objects the declared
policy can reconcile. Grants on other schemas, relations, columns, sequences,
functions, types or defaults reject before role/ACL mutation. Read-only checking
uses the same database-wide dependency check; foreign database/shared-object grants
cannot be hidden by the managed schema's valid ACLs.
Effective schema, table, column and sequence privileges are checked across all
non-system schemas too, including access inherited through `PUBLIC`. PostgreSQL's
system catalogs remain readable. Existing undeclared PUBLIC access requires
explicit operator reconciliation; a newly created dedicated database with a
custom managed schema clears its bootstrap `public` schema grants.

Checks cover schema ownership, role membership, exact table/sequence privileges,
column grants, grant options and defaults for future owner-created objects.
Database, schema, relation and default ACLs reject undeclared grantees. Column
ACLs permit only the owner; explicit runtime privileges belong at table scope.
An unrelated grantee requires explicit operator reconciliation before admission.
PostgreSQL 18 is the qualified privilege surface, including MAINTAIN. Ordinary
provisioning and dependent runtime services retain the PostgreSQL fence startup
condition. A skipped provisioning prerequisite cannot admit a dependent runtime
start while `writer-fence.json` exists. Intentional
maintenance under a fence uses the declared OS-peer control channel explicitly.

## Executed application backups (version 2)

`projects.<name>.backup` declares a user, a dedicated reader group, backup root,
absolute capture/restore/verify/cleanup commands and bounded execution limits.
The systemd service loads credentials from runtime paths. Its timer is independent
of schema migration or backend selection. `lib.applicationBackup = 2` advertises
protected publication. Physical cluster backups remain a separate `pgBackup` contract.
The service requires the mount containing its backup directory, including mounts
configured with `nofail`, before it can capture or publish a restore point.
Declared filesystem ancestors also have explicit `Requires`/`After` dependencies,
so masking their units cannot remove the storage requirement through mount discovery.

Scheduled captures run serialized retention before and after capture. The default
`backup.retention.keepSuccessful = 96` retains the newest 96 timer restore points
plus the point named by `LAST_SUCCESS`. `incompleteAgeSeconds = 604800` expires
incomplete timer attempts after seven days; it must exceed all five capture/retry
stage budgets plus termination slack. Pruning uses the same exclusive backup
lock as capture, so active inspection and transition leases block deletion.
The timer explicitly invokes `capture --scheduled`, generating
`timer-YYYYMMDDTHHMMSSZ-PID`. These attempts and their known partial, sealed,
credential, intent and restore-workspace siblings belong to the timer namespace.
Automatic manual captures use `manual-YYYYMMDDTHHMMSSZ-PID`. Other names, including
every `transition-*` capture, interrupted transition and legacy unprefixed
timestamp/PID attempt, remain operator-retained even while no transition process
holds a lease. Legacy origin is ambiguous, so pruning cannot infer it retroactively.
Use `--attempt ID` to retry an existing timer attempt; `--scheduled` selects a new
attempt and cannot be combined with `--attempt` or `--retry-incomplete`.
Explicit attempt IDs reserve `LAST_SUCCESS` and `lock`. Both capture engines
reject these names before acquiring the mutation lease, creating staging output
or running an adapter, including retries on a fresh repository. Rejection leaves
the existing marker and lease inode intact, and later valid captures can publish.
Scheduled services use `TimeoutStartSec = infinity`: per-stage adapter deadlines
still apply, while full-tree protected publication and both pruning passes may
take longer than a fixed coordinator allowance. Systemd does not kill valid
copy/retention work just because the adapter stages used their configured budget.
For explicit retention use `harbor-db-application-backup --config MANIFEST prune
--keep N --incomplete-age-seconds SECONDS`. Redirected top-level attempts, invalid
success markers and mounted deletion subtrees fail the plan before any deletion;
deletion never follows links in abandoned worker output. Manifest/executable
binding and immutable accepted bytes are preserved for retained restore points.
Known incomplete siblings of a completed timer capture are removed under that
same lease, including when `LAST_SUCCESS` retains the completed point. Its
acceptance evidence and retry intent remain intact; partial/sealed trees,
credentials, restore workspaces and abandoned worker outputs are obsolete.
Selected successful deletions first rename the point durably to `.pruning`.
Retries finish these tombstones and their siblings immediately, even if the
acceptance file was already removed; the tombstone is removed after sibling
cleanup so interruption cannot restart the incomplete grace period.

Generated backup manifests use version 2 and declare `worker_user`. The coordinator
and backup namespace are root-owned; adapters run as the declared unprivileged
account with private per-attempt credentials and workspaces. Verified artifacts
are copied through non-following directory descriptors to fresh root-owned inodes,
checked again, and published as `0440` files beneath `0750` reader directories. The
backup account cannot change modes or replace entries, and a writable descriptor
retained from staging cannot modify the published copy. `LAST_SUCCESS` and the
persistent lease also remain root-owned. Enrolled readers retain group read access.
Regular artifacts are streamed completely with bounded copy buffers; publication
has no implicit 1-GiB file-size limit. Credential files retain their separate
explicit 1-MiB limit.

Version-1 manifests remain compatible with self-operated library fixtures; their
owner controls publication storage. They do not meet the protected-publication
contract. Moving a consumer to version 2 changes the manifest binding and requires
a fresh accepted capture; old receipts cannot be reused under the new manifest.
Independent copied-backup certification runs as an unprivileged account on the
certifier host. Native and Python peers implement the same versioned contract.

Enabled backup project names must match `[A-Za-z0-9_-]{1,128}`, the runtime
resource contract. Invalid names fail NixOS evaluation before a timer is installed.
Every enabled project must also have a dedicated, normalized absolute backup
root, preserving exclusive lock, attempt and success-marker namespaces. Disabled
projects do not reserve roots. Ambiguous dot/parent components and repeated or
trailing separators are rejected before comparing roots. Roots must be pairwise
non-ancestral so another project's directory ownership cannot block traversal.
Nix admission also keeps each enabled backup root disjoint from every enabled
cutover corpus and authority-state namespace, including enrolled PostgreSQL
primary, lifecycle state and startup-inhibition roots, in both containment directions.
Transition admission also keeps the selected backup root disjoint from the
shared source/target authority state in both containment directions. Otherwise
the nested backup coordinator could contend with the transition's own authority
lock while writers remain inhibited. Native and Python reject before inhibition
or lock creation, including when the backup policy is supplied externally.

Backup inventories require UTF-8 relative artifact paths; non-UTF-8 file or
directory names are rejected before acceptance publication. Regular artifact
files must have exactly one hard link; shared inodes are rejected
before changing publication permissions and during accepted-point inspection.
This prevents a mutable external pathname from modifying an accepted artifact.
Every stage requires a literal `{backup}` argument; restore, verify and cleanup
also require `{workspace}`.
Additional `executableFiles` require normalized immutable Nix store paths during
evaluation. The backup manifest is constructed only after every declared file
resolves to a regular file inside the immutable store; directories, missing files
and mutable link targets fail realization before timer installation. Store-local
links retain native/Python file-hashing compatibility. Evaluation does not realize
adapter packages.

Transition `executableFiles` use the same evaluation and realization contract.
Across all enabled provisioning projects, runtime OS accounts must be disjoint
from the complete owner-account set so peer mappings cannot elevate another
project's runtime account. Shared owner-only accounts remain supported.
Embedded fragments such as `--backup={backup}` do not satisfy this contract.
Credential IDs use `[A-Za-z0-9_.-]{1,255}`, excluding `.` and `..`, and credential
paths remain runtime paths outside the Nix store.
Generated version-2 manifests bind those credential names and runtime source paths.
Scheduled captures use systemd's loaded credentials; forced transition captures
resolve the same declared sources directly. Both coordinators copy only the selected
credentials into a private worker-owned directory with `0400` files, bounded to
1 MiB each, and remove it when the attempt finishes. Runtime secret aliases such as
agenix paths are resolved before opening a regular file. Credential contents are
never serialized into the policy or acceptance evidence.

Arguments equal to `{backup}` and `{workspace}` are substituted as single argv
elements, without a shell. Capture creates a new directory and writes all artifacts,
plus `capture.json` with exactly `version = 1`, `consistency` (`quiesced` or
`shared_exported_mvcc_snapshot`) and `semantic_sha256`. The application chooses
the canonical complete-record representation whose hash identifies this snapshot.
Restore targets only the newly allocated private workspace. Verify emits exactly
`version = 1`, `status = "verified"` and the same `semantic_sha256` after executing
complete schema/record/revision/sequence comparison. A row count alone is insufficient.
Cleanup must be idempotent, stop and drain every process it started, and is invoked
even after restoration fails. Failed cleanup preserves its private workspace.

`harbor-db-postgres-drill --package POSTGRES restore|cleanup BACKUP WORKSPACE`
supplies a reusable logical PostgreSQL restore adapter for `database.dump`.
It creates `WORKSPACE/socket`, listens only on that private Unix socket at port
55439, and restores database `harbor_restore`. Applications supply their semantic
verifier, which connects to this disposable endpoint. No live database is dropped.
The owning backup adapter supplies `timeoutSeconds` through
`HARBOR_DB_APPLICATION_TIMEOUT_SECONDS`. Restoration shares that complete budget
across initialization, startup, database creation and `pg_restore`; individual
children do not receive fresh probe deadlines. Standalone calls default to 1800
seconds and may select `--timeout-seconds` from 1 to 86400, overriding inheritance.
Failure attempts bounded cleanup separately. Abrupt coordinator cancellation
retains the private workspace for explicit `cleanup` or managed retry; SIGKILL
does not establish automatic cleanup.
Cleanup treats `postmaster.pid` as an untrusted reference. On Linux it pins the
process with a pidfd and verifies its PostgreSQL executable, worker identity,
cluster working directory, process-group/session leadership and start time
before sending fast-shutdown SIGINT through that descriptor. Numeric PID reuse
cannot redirect the signal. A confirmed dead process permits compare-bound,
directory-synchronized removal of its stale PID file. A live foreign process or
changed carrier rejects and retains evidence; no `pg_ctl stop` is invoked from
the persisted numeric PID. The same bounded cleanup protocol works across engines.

`harbor-db-application-backup --config MANIFEST capture [--attempt ID]` records
every regular-file artifact and executable hash and every directory path (including
empty directories), executes restoration and verification, and flushes the accepted
tree. Artifact and acceptance files publish with read-only `0440` owner/group
permissions; reader directories retain `0750` traversal permissions. The worker
cannot grant itself write access to version-2 published storage; every admission
also verifies digests. Capture freshness is rechecked immediately before publishing the
tree and advancing `LAST_SUCCESS`. A capture that aged past `maximumAgeSeconds` leaves
partial evidence and preserves the previous marker, allowing an explicit retry
of the same attempt. Publication
sets the marker's final `0640` reader permissions and file-syncs them before
rename, then syncs the containing directory. An interruption
can leave a valid unpublished point; the previous success remains usable. Attempts
are never overwritten. `inspect BACKUP` rechecks bytes, contracts and freshness.

`certify BACKUP --state PRIVATE_STATE` executes a copied backup on another machine.
The private state directory and persistent `lock` inode must already exist. Its
receipt binds the original acceptance hash, actual machine identity, executables
and semantic identity. Execution on the original machine cannot claim independence.
If a caller loses the response after receipt publication, retry executes restore
and all byte/tool/semantic checks again, then returns the matching retained receipt
with its original `certified_at`, bytes and inode. Conflicting evidence or a
timestamp outside the source-capture/current-time interval rejects without
rewriting the receipt. Native and Python peers can resume one another's result.
Copying bytes or trusting an application's unexecuted assertion is insufficient.
Adapters must retain inherited leases through child work and drain descendants
before returning. Native and Python bounded workers kill surviving process-group
descendants before reaping even a successful leader, so silent descendants cannot
retain leases past reported completion. Generated systemd units additionally retain
control-group cleanup.
The coordinator forwards its leases to every adapter through inherited descriptors
listed in `HARBOR_DB_LEASE_FDS`; adapters forwarding work must retain them too.
Receipt output is bounded in memory; diagnostic output is discarded rather than
spooled into persistent files. This does not authorize secrets in artifact dumps.
Operator commands use canonical manifest paths. Resolve generated `/etc/harbor-db`
convenience links to their store paths before passing them to a no-follow reader.

An interrupted point may be retried explicitly with `--retry-incomplete`. Its
manifest and executable hashes must match the original attempt. Cleanup drains
the old disposable restore first; partial bytes and intent are moved into a private
abandoned directory before a fresh capture. Existing accepted points are immutable.

## Backend transitions (version 1)

Independent restore receipts must use normalized, non-redirected paths outside
authority identity/journal/lock/custody files and all barrier/systemd control
namespaces. Both engines reject aliases before acquiring transition locks or
inhibiting writers; the selected custody file is reserved even with a custom name.
Receipts must also be disjoint from the selected backup publication root, including
its lock, success marker, restore points and attempt workspaces. Keep independent
restore evidence in a separate directory so admission can reject control aliases
before stopping writers.

`projects.<name>.transition` and `lib.applicationBackendTransition = 1` expose the
shared journal engine. Source and target resource manifests identify one authority
directory, retained storage roots and different backend bindings. Applications
supply import, complete source/target parity and post-release health workers.
Commands run under explicit service accounts and receive single argv substitutions
for `{backup}`, `{source}`, `{target}` and, after binding, `{candidate}`.
Every worker requires nonempty argv with an absolute executable; the generated
NixOS configuration rejects invalid workers before installing transition tooling.
Enabled transition-only project names also use `[A-Za-z0-9_-]{1,128}`.
The nested capture runs as the owner of the validated, non-redirected backup
root, which may differ from the authority-state owner. Each adapter retains its
backup deadline; the outer coordinator allows five such budgets (retry cleanup
plus capture, restore, verify and cleanup) and 60 seconds of termination/IO slack.
Operator policy references must be immutable store files, including non-null
PostgreSQL and custody manifests. Store-internal aliases retain their original
contract spelling; mutable-location aliases are rejected before dispatch.
Custody inventories reject regular files with external hard links in metadata
and content checks. Database inventory queries supply paths and attributes;
the policy's root index remains authoritative and query-supplied `root` fields
are rejected in both ordinary cutover and transition custody publication.
Retried preparation and commit compare the complete persisted custody object
against reconstructed evidence, excluding only `completed_at` from equality.
The retained timestamp must still be a positive integer no later than the fresh
verification. Missing fields, added fields and non-object custody reject before
authority or custody publication.

The root coordinator installs persistent systemd client barriers before stopping
writers and taking the exclusive authority lease. Legacy service units are included
explicitly. PostgreSQL transitions borrow the existing capability-3 writer fence;
only local OS-peer control SQL remains available. An importer must use that control
path, for example a `postgres`-owned worker using `SET ROLE` for the dedicated
schema owner. It cannot open application HBA access or release the borrowed fence.
No transition command starts PostgreSQL or application services.
Across enabled projects, each active or retired writer unit belongs to one
transition authority. Barrier directories are dedicated normalized namespaces,
pairwise non-overlapping with each other and enabled backup roots, so persistent
start policies and lock anchors cannot be shared or shadow another project's
ownership. Nix admission checks barriers against every enabled cutover corpus,
authority state, and enrolled PostgreSQL primary/lifecycle/startup state too. Disabled
projects do not reserve units or namespaces.
Writer release and abort validate every selected inhibition marker before updating
`start-policy.json` or removing any marker. Rejection preserves the retained startup
policy and all barriers; interrupted successful releases remain retryable.
The barrier must also be disjoint from source and target storage roots and the
authority directory in both containment directions; it cannot be their ancestor.
The selected backup root must also be disjoint from both source and target corpora
and the transition barrier in either containment direction. Runtime manifest
admission enforces this even for an external immutable backup manifest outside
the enabled project backup set, before writers are inhibited. Filesystem custody roots are globally disjoint
across resources so a corpus has exactly one resource lease.
Every runtime writer declared by the custody policy must belong to the
transition's active or retired unit set. Both engines enforce this in manifest
admission before preparation and recheck it before inventory, so custody cannot
certify a corpus whose declared writer escaped the transition barriers.
Borrowing requires the owner's existing persistent fence, backup-mutation and
recovery-evidence lock anchors. Provision those anchors through the owner before
application preparation, including contract-first preparation before the first
post-import snapshot; the application coordinator does not create owner anchors.

The sequence is:

1. `plan --candidate CONTRACT [--writer-fence-token TOKEN]` binds a realized
   immutable transition manifest before building the full system closure. A fresh
   128-bit `run_id` is persisted in the journal before capture. The attempt name
   binds both intent and run identity, so intervening source writes after abort and
   retirement are captured by the next run. Resuming an existing journal retains
   its ID and exact attempt; pre-ID legacy journals keep their original attempt.
2. `prepare` installs and verifies effective client barriers, drains writers,
   captures/restores the source, and waits for independently executed restore
   evidence. A repeated call uses the same immutable intent and retained snapshot.
   Startup and reload commands remain admission-guarded. `ExecStop` and
   `ExecStopPost` retain their original commands so graceful shutdown and cleanup
   can finish while the journal is `quiescing` and ordinary startup is inhibited.
3. Import runs once per successful preparation; interrupted imports must resume
   idempotently for that snapshot. Whole-primary PostgreSQL recovery evidence is
   refreshed by the owning recovery coordinator **after** import. Preparation
   retains phase `imported` while that external acceptance is pending; retry does
   not re-import successful target data. Existing physical recovery policy is
   retained, including off-host evidence when required.
4. `bind-candidate --candidate GENERATION` binds the exact realized system after
   successful preparation. Mandatory preflight admits the prepared target with
   writers inhibited. Pre-switch admission additionally compares the actual
   NixOS-supplied generation path, preventing a different system with the same
   manifest from using the receipt.
5. Activate the bound generation using the consumer's existing activation lease.
   `commit` revalidates independent proof, complete parity, target corpus and
   token-bound whole-primary recovery evidence. Authority and target corpus
   custody publish through a journaled compare-and-swap. Client barriers remain.
6. `enable-writes` revalidates evidence and persists the point of no return before
   removing client inhibition. The root-owned startup policy admits only the exact
   released generation, including legacy units without a resource wrapper.
   Retired unit names remain inhibited. The borrowed cluster fence must still be
   released explicitly by its owner; then start clients and run real acceptance.
   If interruption leaves `write-enabled` with its inhibition marker present,
   retry keeps exclusive authority and the source backup pin, then repeats fence,
   recovery, independent proof and target-custody checks before removing the marker.
   A completed release is idempotent and permits subsequent acknowledged writes.
7. `complete` checks health and authority without requiring equality to old
   records or taking an exclusive lease from the now-running writer.
   A retained inhibition marker blocks completion and retirement.
8. `retire` archives terminal journal evidence before another transition may be
   planned. History filenames bind both the immutable intent and complete terminal
   record, retaining distinct runs of one intent. Resuming retirement after archive
   publication preserves that archive's bytes and inode. The root-owned generation
   startup policy remains retained. Authorizing
   a new backend or generation uses another explicit qualified transition.

Barrier installation validates every existing primary and retired inhibition
marker before provisioning startup policy or any unit drop-in. A foreign or
malformed marker preserves the retained policy, markers and drop-ins, including
their bytes and inodes, and cannot install conditions for new units. Release and
abort use the same ownership check before policy publication or marker removal.
An owned installation can be retried after partial publication.

Independent transition receipts must be outside every source and target corpus
in both containment directions, as well as backup publication and transition
control storage. Equality, descendants and enclosing roots reject during manifest
admission before locking, capture or writer inhibition; separate evidence siblings
remain supported.

Before writer enablement, `abort` requires the exact retained source generation
selected with clients still inhibited and intact source semantic evidence. It
restores old authority/custody using compare-and-swap, preserving target bytes.
The journal-bound rollback capture may be older than `maximumAgeSeconds`; abort
still verifies its complete artifacts, acceptance digest, executable/manifest
identity and source semantics. Invalid or future capture timestamps reject.
Forward admission and independent certification still require fresh evidence.
After writer enablement, abort is forbidden even if no write has yet been observed:
reversal requires a fresh reverse transition. Release and abort can resume after
an interruption; neither rewrites foreign identity or startup policy.

Consumers using mandatory filesystem/corpus admission provide
`targetCustodyManifest` (a target resource entry without `transition_manifest`) and
set `cutover.resources.<name>.transition_manifest` to the generated transition
manifest. The shared engine publishes target custody at authority commit; normal
startup continues through the existing resource/cutover guard. Database-owned
corpus references are queried read-only and validated against complete target bytes.

Qualification retains all existing Harbor DB gates and adds dedicated provisioning
evaluation and four systemd/VM application gates. Native regressions cover changed
ownership/default/column privileges, real PostgreSQL restores, failed publication,
changed evidence, surviving child leases, interrupted authority commit, generation
binding, source-preserving abort and forbidden rollback after writer enablement.
The PostgreSQL transition gate executes a real importer under the borrowed fence,
retains a successful import while whole-primary acceptance is pending, performs a
physical restore at the same fence token, and rejects record drift. Aborting the
application transition leaves that primary fence intact for its owner's explicit
stopped-primary release. Independent application restoration executes on a second
machine; the filesystem transition gate separately exercises exact-generation
activation, client release, post-write health and retained legacy startup policy.
