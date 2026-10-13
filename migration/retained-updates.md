# Retained updates after the initial import

This record supplements `retained-work.md`, whose 112-commit coverage is bound
to the initial `imported-ref` namespace. These later tips are preserved under
`refs/tags/retained-ref/20261012/`. The dispositions below describe the active
source reconciled after `14ef53b1aec5a398e802514cdd1197d54e1c54d0`; producer
qualification and consumer acceptance remain separate gates.

## Rust sandbox

The retained tip is `ae824784966c95e3d16278bdcdd9b60c1502b26a`, archived as
`harbor-rs/remotes/origin/feat/dev-sandbox-20261002`. Its boundary is
`f2ca8231d1430f6e1f55deef079fccc6fa24bb98` and it adds five commits.

| Commit                                     | Disposition                        | Active evidence                                                                                                                                                                                                                                          |
| ------------------------------------------ | ---------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `41d6bad9ef12f3f3043009a07744bee41225e2ce` | Incorporated                       | `components/rust/crates/harbor-sandbox/`, `cli/src/commands/sandbox.rs`, `lib/dev-sandbox.nix`, and `docs/src/api/dev-sandbox.md` retain isolated source copies, private state, declared desktop capabilities, and readiness-gated warm replacement.     |
| `5f951111c5cb5e26725ad9c038854866807e9cc2` | Incorporated with shared CI wiring | `nix/checks-dev-sandbox.nix`, the Rust component's flake checks, and root `simit.toml` retain the runtime and exact-output gates. The root workspace and component workflows replace the predecessor's standalone workspace and per-crate CI.            |
| `c736596031ad21ccc29d43c253f214da8060295c` | Incorporated                       | `nix/dev-shells.nix` retains the launcher changes; `crates/harbor-sandbox/src/process.rs` retains supervised preview dependencies. Both files match the retained tip byte-for-byte.                                                                      |
| `ec2a17a44e8286b8170d86190fa2cea379ccb965` | Absorbed merge                     | Its sandbox implementations and shared CI policies are covered by the preceding rows. The predecessor merge remains reachable through the archived tip.                                                                                                  |
| `ae824784966c95e3d16278bdcdd9b60c1502b26a` | Pending publisher integration      | The root package-qualified publisher still executes publication from its tag workflow. The retained signed-source/trusted-publisher split must be regenerated through a hosted-accepted full Simit producer and qualified before predecessor retirement. |

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
package-only selection while the publisher-boundary update is pending.

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
The integration retains 48 writer-fence Rust runtime files byte-for-byte,
11 original writer-fence Python methods with identical ASTs, and all 31 frozen
PR14 files without formatting changes. The 548-selector suite preserves all
415 selectors from the native tip and all 538 from the writer-fence tip.
Enrollment metadata remains separate from restart-persistent runtime fencing;
mixed enrollment/runtime schemas require explicit conversion before publication.

The uncommitted source snapshot passed all 35 database checks and nine production
package selections, including the recovery and transition VMs. Canonical-source
and hosted qualification remain pending. The archived tips and these snapshot
results preserve implementation and evidence; predecessor retirement still
requires canonical acceptance and verified consumer migration.

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
