# harbor-cad

<!-- simit:badges:start -->

[![CI](https://img.shields.io/badge/CI-managed-2088ff)](.github/workflows/ci.yaml)

<!-- simit:badges:end -->

Local-first Rust CLI/worker and narrow Python MCP for scientific CAD jobs.
The authoritative scope is [implementation-spec.md](docs/implementation-spec.md).
The current implementation is **pre-qualification**: the CPU analytical-reference
workflow is executable; native/GPU adapters require separate qualification.
Process success is never a physical-validation claim.

## Run the implemented reference workflow

```sh
cargo build --locked --jobs 2
uv sync --locked
harbor-cad worker --state /absolute/private/state --profile profiles/ci.json
harbor-cad case init > case.json
harbor-cad case plan case.json > planned.json
```

`case plan` returns `{approval_digest, plan}`. Save `plan` as `plan.json`, then:

```sh
harbor-cad --socket /absolute/private/state/worker.sock job submit plan.json \
  --approve <approval_digest> --idempotency-key reference-001
harbor-cad --socket /absolute/private/state/worker.sock job status <job-id>
harbor-cad --socket /absolute/private/state/worker.sock results describe <job-id>
harbor-cad artifact export --state /absolute/private/state <job-id> ./portable
```

The CI profile uses explicitly weaker foreground execution and permits only
analytical references and bundle indexing. Native stages require systemd job
services and exact Nix-packaged sandbox/runtime paths. Host drivers and GPU
permissions are operator-owned; nothing installs drivers or uploads artifacts.

MCP uses the same worker, with bounded structured responses and durable job IDs:

```sh
HARBOR_CAD_SOCKET=/absolute/private/state/worker.sock \
  uv run --locked harbor-cad-mcp --profile all
```

Profiles: `cad`, `simulation`, `results`, `all`. There is no arbitrary shell,
Python evaluation, package installation or scientific-array response tool.

## Verification

```sh
cargo test --locked --jobs 2
cargo clippy --locked --all-targets --jobs 2 -- -D warnings
HARBOR_CAD_TEST_BINARY="$PWD/target/debug/harbor-cad" uv run --locked pytest -q
treefmt --config-file treefmt.toml
```

`doctor` is read-only inventory, not a GPU benchmark. `qualify` reports gate
status without promoting untested combinations. Native package outputs and
execution receipts must be qualified before A0/B1 can be declared complete.

## Native plans and qualification

`case plan-openlb-reference case.json --policy research` prepares the explicit
FreeCAD → CPU OpenLB → bundle path. The case must declare the synthetic
`periodic_forced_channel` formulation and satisfy its fixed low-Mach limit.
`case plan-b1 case.json --devices devices.json --policy research` prepares
FreeCAD → required HIP or CUDA OpenLB → independent EGL → VAAPI → bundle.
`devices.json` follows the generated `B1Selections` schema; compute requires
an explicit PCI identity and backend UUID. HIP/ROCm is the primary AMD path;
CUDA is best-effort. See the [backend research decision](docs/gpu-backends.md).
Planning is independent of runtime
qualification; submission rechecks devices and the configured backend.

MCP exposes `case_plan_openlb_reference` and `case_plan_b1` through the same
worker, using its host policy. Both require a systemd/native-capable profile.
The packaged synthetic CPU workflow has passed. The production CAD importer
has also passed the scoped isolation checks described below. GPU and other
native-stage policies require their own execution evidence.

`openlb-hip` builds the pinned OpenLB HIP implementation for explicit `gfx1100`
with Float64. `runtime-hip` describes the CAD/solver/render/media packages;
its presence does not enable unqualified KFD worker isolation. The HIP driver
exposes `--gpu-inventory` for PCI/UUID correlation, and
`scripts/verify_openlb_hip.py` checks the procedural channel, retained CPU/HIP
field agreement and rejected identity/fallback cases. Inventory is not kernel
evidence. These probes are separate from complete CLI/MCP B1 qualification.

Measured CPU and systemd results, limitations, exact source/patch identities
and Canix package validation are recorded in
[qualification.md](docs/qualification.md) and
[dependency-manifest.json](docs/dependency-manifest.json).

The CPU native runtime excludes renderer/media closures. It records the exact
runtime manifest and SI-normalized native plan as exportable artifacts. Imported
CAD is copied from one bounded descriptor, verified against its approved digest,
and exposed read-only; subsequent changes to the original file cannot alter the
importer's input. Every native stage requires a fresh matching adapter/backend
receipt with actual execution and no software fallback.

Systemd jobs also require the live unit's invocation, main PID and exact cgroup
membership; an inherited environment value alone cannot authorize execution.
The verified identity is archived as `service-owner.json`.

Jobs bind the exact packaged runner and native runtime identities before
acknowledgement. Queued execution preserves that runner across worker upgrades;
idempotent retries reuse the binding. Active systemd jobs retain their closures
with durable, job-scoped Nix GC roots. Cleanup waits for verified complete-tree
termination. See [execution lifecycle and compatibility](docs/execution-lifecycle.md).

The patched importer now mounts only its declared package closure read-only and
checks the effective isolation before opening a document. The production
worker qualifier verifies immutable inputs, live synthetic credential/session
and host-loopback canaries, absent GPU nodes, resource controls and detached
descendant cleanup on success and region rejection:

```sh
python scripts/verify_importer_policy.py \
  --executable /nix/store/CLI/bin/harbor-cad \
  --runtime /nix/store/RUNTIME-harbor-cad-native-runtime.json \
  --source /absolute/controlled-fixture/source.FCStd \
  --output /absolute/new/importer-qualification-directory
```

The fixture must contain the named `fluid` solid. Exact-pin advisory review is
recorded in [FreeCAD security evidence](docs/evidence/freecad-security.json).
Encoded videos now bind a checksummed frame sequence to the approved physical
times and completed render receipt; see [the frame contract](docs/frame-sequence.md).

`case plan-cad-inspection case.json --max-artifact-bytes 67108864` prepares a
CAD-only `cad_inspect → bundle` plan. Set `geometry.source` to a path relative to
the worker's allowed input root and `geometry.sha256` to its lowercase SHA-256.
The CLI returns `{approval_digest, plan}` for ordinary approved job submission;
planning opens no document. MCP `cad_plan_inspection` exposes the same Rust
planner in the `cad` and `all` profiles. This inspection does not launch a solver.

`checks.x86_64-linux.openlb-cpu-reference` runs the procedural-STL numerical
reference using the packaged solver. For opt-in FreeCAD integration, an existing
systemd user manager and the separately built CLI, MCP and `runtime-cpu` are
required:

```sh
uv run --locked python scripts/verify_native_cpu.py \
  --executable /nix/store/CLI/bin/harbor-cad \
  --mcp /nix/store/MCP/bin/harbor-cad-mcp \
  --runtime /nix/store/RUNTIME-harbor-cad-native-runtime.json \
  --output /absolute/new/qualification-directory
```

This verifier submits resolutions 8 and 16 through the CLI and the official MCP
client, checks retained fields and every exported checksum, and keeps physical
validation explicitly unqualified. Its packaged run passed with velocity errors
of 1.1124% and 0.2781%, respectively; each bundle has 28 verified records.

`scripts/verify_vaapi.py --runtime /nix/store/RUNTIME.json --media
/nix/store/MEDIA/bin/harbor-cad-video --pci 0000:03:00.0 --output /absolute/new/path`
performs an opt-in three-frame 128×128 synthetic encoding/decode probe. The
selected Radeon RX 7900 XTX passed with H.264 VAAPI and actual CPU decoding.
Only selected read-only DRM/PCI metadata accompanies the render node; other
cards, PCI config/resources and session paths stay hidden. This probe does not
establish EGL, physical-time-label correctness or the HIP-first B1 workflow.

`scripts/verify_native_recovery.py --executable /nix/store/CLI/bin/harbor-cad
--runtime /nix/store/RUNTIME.json --output /absolute/new/path` checks actual
OpenLB partial-field recovery after owned-service SIGKILL plus worker restart
and cancellation. Both packaged runs preserved the exact initial Float64 VTI
in 23-record checksummed bundles, with failed/cancelled execution status.

Retained resolution-16 CPU OpenLB fields also passed selected-device EGL surface
rendering and VAAPI encoding/CPU decode on the RX 7900 XTX. The three 640×480
decoded frames preserve units, a fixed velocity scale and 0/10/20 s labels.
Native fields remain unchanged; durable HIP OpenLB/worker B1 is unqualified.
`scripts/verify_egl_fields.py` and `examples/native_stage_probe.rs` reproduce this
opt-in adapter probe with production DRM containment and shared card reservations.
See [the rendering recipe and measured evidence](docs/qualification.md#retained-field-egl-surfaces-and-vaapi-video).

Artifact listing returns `{items, total, next_after}` with at most 100
descriptors and 24 KiB of descriptor data per page. Use
`artifact list JOB --after PATH --limit 20` or MCP `artifact_list` with the
returned `next_after` until it is null. `results describe` embeds the first
page. Wait for a terminal job state for a stable traversal; an active job can
still register files. Export always includes the complete registry regardless
of page size.

Terminal failed/cancelled jobs can also be exported. Every export includes
`execution.json`, containing the terminal state, original approved plan and
host profile, with its checksum in `manifest.json`. Exporting a failed
attempt preserves diagnostics without implying scientific qualification.
Historical inputs stay unchanged, and `current_plan_check` records whether
the current applicability gate would accept their execution.
Native failures snapshot regular closed outputs under `failed-native/`,
including opaque partial files, and register `native-failure.json`. Unsafe or
over-budget entries are identified as omissions and remain in the private raw
tree. After confirmed owned-service termination, restart reconciliation and
cancellation register bounded closed raw records before the terminal state.
Recovery errors are included in the terminal diagnostic and retain the raw tree.

Workers may opt into the versioned [host authority and shared admission](docs/host-authority.md)
with `worker --authority /absolute/authority.json`. This binds exact route/device
authorization and uses one durable same-user RAM/filesystem/card reservation
journal before service launch. Historical approvals retain their identities.

The legacy worker defaults to one admitted plan per state root. It reserves the
declared peak RAM and full output allowance; native jobs additionally reserve
space for a verified staging copy. Existing retained artifact bytes count
against that root's disk budget. Insufficient occupied capacity keeps a job
queued; a plan exceeding the host's total allowance is rejected. Scientific
parameters remain as approved.

Each submission atomically binds an immutable host profile to its plan.
The service reads a sealed copy and exports `host-profile.json`; changing the
configuration file cannot change a queued or running job. Reusing an
idempotency key with a different plan or profile is a conflict. Systemd's
effective `MemoryMax` is the plan's peak estimate, within the host ceiling.
Legacy jobs without a recorded profile remain readable and exportable, but
cannot be relaunched automatically under a guessed profile.

GPU jobs share exclusive physical-card anchors under
`/run/user/UID/harbor-cad/cards` across state roots. Compute, render and media
selections on one PCI card share one reservation. A job holds its files across
stages independently of the worker, and waits before executing if another job
holds a card. Host RAM and artifact budgets are scoped to each state root;
cross-root aggregate RAM/VRAM admission remains a separate qualification gate.

The Simit-generated hosted CPU workflow runs locked Rust/Python checks and the real MCP client
on public GitHub-hosted runners, with commit-pinned actions and read-only
repository permission. Native Nix builds and opt-in systemd/GPU qualification
are recorded separately in [qualification.md](docs/qualification.md).

`python3 scripts/check_cpu.py` runs the same bounded CPU gate locally.
`simit.toml` owns the workflow; generation uses Simit revision
`afb7939d925d3e8e9b8507387ada7efad6460df8` because the installed 0.19.0
binary predates `[ci].check_command`. Regenerate and check with that source's
`simit init ci --platform github --runtime cargo [--check --diff]`.
