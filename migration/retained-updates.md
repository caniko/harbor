# Retained updates after the initial import

This record supplements `retained-work.md`, whose 112-commit coverage is bound
to the initial `imported-ref` namespace. These later tips are preserved under
`refs/tags/retained-ref/20261012/`, `refs/tags/retained-ref/20261013/`, and
`refs/tags/retained-ref/20261014/`.
The dispositions below describe the active
source reconciled after `14ef53b1aec5a398e802514cdd1197d54e1c54d0`; producer
qualification and consumer acceptance remain separate gates.

## Rust sandbox

The retained tip is `ae824784966c95e3d16278bdcdd9b60c1502b26a`, archived as
`harbor-rs/remotes/origin/feat/dev-sandbox-20261002`. Its boundary is
`f2ca8231d1430f6e1f55deef079fccc6fa24bb98` and it adds five commits.

| Commit                                     | Disposition                        | Active evidence                                                                                                                                                                                                                                                                                                |
| ------------------------------------------ | ---------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `41d6bad9ef12f3f3043009a07744bee41225e2ce` | Incorporated                       | `components/rust/crates/harbor-sandbox/`, `cli/src/commands/sandbox.rs`, `lib/dev-sandbox.nix`, and `docs/src/api/dev-sandbox.md` retain isolated source copies, private state, declared desktop capabilities, and readiness-gated warm replacement.                                                           |
| `5f951111c5cb5e26725ad9c038854866807e9cc2` | Incorporated with shared CI wiring | `nix/checks-dev-sandbox.nix`, the Rust component's flake checks, and root `simit.toml` retain the runtime and exact-output gates. The root workspace and component workflows replace the predecessor's standalone workspace and per-crate CI.                                                                  |
| `c736596031ad21ccc29d43c253f214da8060295c` | Incorporated                       | `nix/dev-shells.nix` retains the launcher changes; `crates/harbor-sandbox/src/process.rs` retains supervised preview dependencies. Both files match the retained tip byte-for-byte.                                                                                                                            |
| `ec2a17a44e8286b8170d86190fa2cea379ccb965` | Absorbed merge                     | Its sandbox implementations and shared CI policies are covered by the preceding rows. The predecessor merge remains reachable through the archived tip.                                                                                                                                                        |
| `ae824784966c95e3d16278bdcdd9b60c1502b26a` | Incorporated publisher split       | `publish-crate-harbor-sandbox.yaml` builds a credential-free signed-source archive; `trusted-publish-crate-harbor-sandbox.yaml` verifies and publishes through the independent default-branch engine. Exact integrated-source qualification and package-specific publication remain separate retirement gates. |

Thirteen engine, fixture, documentation, and Nix files match the retained tip
byte-for-byte: the sandbox command; all three sandbox engine files; the test
helper, contracts, preview fixture, nested tests, and runtime tests; the sandbox
API documentation; `lib/dev-sandbox.nix`; `nix/checks-dev-sandbox.nix`; and
`nix/dev-shells.nix`.

The remaining integration changes preserve the sandbox's `0.1.0` version and
`MIT OR Apache-2.0` license, make former workspace metadata explicit, and point
the repository metadata at the monorepo. The CLI exposes its sandbox command on
Linux. Root `Cargo.toml` owns workspace membership; the root lock and generated
workflows own dependency resolution and publication. Root
`publish-crate-harbor-sandbox.yaml` retains the `harbor-sandbox/v*` namespace and
package-only selection. The split was integrated with the qualified `90bed59`
generator and retained by the `54ae206` diagnostics successor; see
`bootstrap.md` for current enrollment and release scope.

## Database lifecycle and recovery

The boundary is `38ebf2cfbca678cc1ab79a96d6968002fb382b5e`. Two later retained
tips share thirteen commits and preserve 76 distinct additional commits:

| Archived ref suffix                                     | Tip                                        | Commits beyond boundary |
| ------------------------------------------------------- | ------------------------------------------ | ----------------------- |
| `harbor-db/heads/feat/native-lifecycle-v28-20261010`    | `04bb467215967f2b88bf28fb975c37dab8949a39` | 15                      |
| `harbor-db/heads/feat/postgresql-writer-fence-20261008` | `784f80a62adedcaf04c796cc6e4866955e09d5bc` | 74                      |

The active database component now incorporates both retained tips: the
`src/storage/`, `src/testing/`, and `src/planner/` engines, application
provisioning/backup/transition adapters, source-local recovery, and supervised
qualification suite. `database-source-reconciliation.json` binds the integrated
file inventory, both-tip dispositions, and selector execution contracts.

The writer-fence tip includes the native-default rollout and subsequent fixes
for custody namespaces, backup publication, restored-object admission, writer
authority, transition startup/shutdown, producer slots, and bounded recovery.
The initial database integration retained 48 writer-fence Rust runtime files byte-for-byte,
11 original writer-fence Python methods with identical ASTs, and all 31 frozen
PR14 files without formatting changes. The 548-selector suite preserves all
415 selectors from the native tip and all 538 from the writer-fence tip.
Enrollment metadata remains separate from restart-persistent runtime fencing;
mixed enrollment/runtime schemas require explicit conversion before publication.

The initial uncommitted source snapshot passed all 35 database checks and nine
production package selections, including the recovery and transition VMs.
The engines were then committed in `f14d75f`, with later fixture and shared-shell
repairs retained through `7598b83`. Exact local qualification of that canonical
revision passed the full 35-entry database aggregate and its production
selections. The current advanced-default reconciliation requires its own exact
source and hosted qualification. The archived tips and earlier receipts retain
their original scopes; predecessor retirement still requires accepted integrated
source and verified consumer migration.

### Advanced original DB default

The refreshed original checkout is clean at
`00ae675dd785fde667656361198b6793367ec054`. Its complete history is retained at
`refs/migration-retained/predecessor-harbor-db-00ae675-20261014`. The later delta
from retained writer-fence tip `784f80a` adds bounded capture-control admission,
stable machine identity, protected independent evidence, immutable validators,
credential inode/alias checks, service-account execution, namespace fixtures and
storage/supervisor refactoring.

The active component reconciles that delta while preserving monorepo input and
Cargo ownership, earlier native-tip enrollment helpers, the frozen PR14 oracle,
and the union of original selectors. The production package retains the upstream
static-link contract; fault-injection test packages remain separate. Existing
external-target mounts and the strict immutable-Git fixture coexist with the
new declared kernel/binfmt and store-ownership fixtures. Runtime-extension hashes
bind the formatted active source, with the original baseline hashes unchanged.

`database-source-reconciliation.json` remains the historical two-tip receipt.
The advanced-default delta requires fresh complete native, Nix, production and
hosted qualification before producer admission or consumer cutover.

## Command-scoped core runtimes

The signed predecessor tip `780f01e218d159826e47ae125525f3280954b3e7`
adds one commit beyond `c86dd5f7879aea4a30850af9bcbf5dfadb94e684`.
Its complete original history is retained at
`refs/migration-retained/predecessor-harbor-meta-780f01e-20261013`.
The public archival tag is
`retained-ref/20261013/harbor-meta/heads/feat/shared-timezone-env`; an independent
HTTPS fetch verified the original tip after publication.

`components/core/lib/runtime.nix` and `checks/scoped-runtime.nix` retain the
original command-scoped launcher and runtime/composition assertions. The active
`lib/shell.nix` preserves child dependency inheritance, environment precedence,
and ordered, once-only hooks. Documentation uses the canonical `lib.core`
namespace. Root input and workflow ownership replaces the predecessor's
standalone flake/check wiring. Producer qualification and consumer acceptance
remain separate from this source reconciliation.

The root exposes the portable `core-scoped-runtime` check and includes it in
the Darwin core qualification groups. The retained timezone-consumer fixture
supplies `builtins.all` for the runner's new argument-list assertion.

## Managed Python development environments

The signed tip `685f22e60df4fd9faf3d806f8ded1a8e401823fa` adds one commit
beyond `d8d116ebb3f91603b0780e34c51a72d46b47a0ca` and is archived at
`retained-ref/20261013/harbor-py/heads/trunk`. An independent HTTPS fetch
verified the original tip after publication.

The active `components/python/lib/uv-dev-shell.nix`, `lib/venv.py`,
`tests/test_venv.py`, and `checks/managed-venv.nix` match that tip byte-for-byte.
The Python facade delegates `mkUvDevShell` to the retained constructor; the
root-owned component check includes the managed-venv gate. The integration
preserves the existing TorchCodec compatibility recipe and root dependency
graph rather than importing the predecessor's older standalone definitions.

Native offline regressions and the Linux Nix hook check passed. Four-system
evaluation exposes the managed-venv gate, with native runtime execution still
required on each selected hosted platform before producer admission.

## Native Rust target runners

The signed tip `ad028e0278441392a104dbd0f1126dcc9958c638` adds one commit
beyond `195f8a7e81d67102868631015b6794280b492c88` and is archived at
`retained-ref/20261013/harbor-rs/heads/feat/portable-release-files`. An
independent HTTPS fetch verified the original tip after publication.

The active `components/rust/lib/dev-shell.nix`, `lib/native-runner.nix`, and
`nix/checks/native-runner.nix` match that tip byte-for-byte. Linux component
checks and the native Darwin tool groups expose the retained executable check.
The shell forwards composed `inputsFrom` dependencies to Crane, preserves
cross-target runner variables, and rejects competing native runner overrides.

The Linux Cargo run/test/bench gate passed. Four-system evaluation exposes all
three restored runtime checks; hosted native execution remains a separate
producer-admission requirement. Original predecessor working-tree and index
edits remain preserved outside these integrated signed-commit deltas.

## Publisher-owned static binary packages

The refreshed original Rust checkout adds branch `feat/upstream-static-binaries`
at signed tip `608afb8f1157044f9901b39bdae54f23e0227e95`. Its four implementation
commits after `f2ca823` are `417f596`, `8e303af`, `731830d` and `608afb8`.
The complete history is retained locally at
`refs/migration-retained/predecessor-harbor-rs-608afb8-20261014` and published as
signed archival tag
`retained-ref/20261014/harbor-rs/heads/feat/upstream-static-binaries`.

The active Rust facade exposes `mkUpstreamBinaryPackage`; the constructor retains
the source's path, static ELF, architecture, executable-entry and optional
executable-mode contracts. All five source check groups are wired into Linux
component checks and the existing full Rust qualification lane. The shared
manifest owns platform selection and bounded execution; the predecessor's
standalone generated workflow and lock are not separate owners. The existing
manifest-based release constructor remains covered by the retained legacy check.
The imported Python ELF fixture follows the shared formatter while retaining its
fixture semantics. Source integration and archival publication do not establish
hosted acceptance or permission for consumer cutover.
