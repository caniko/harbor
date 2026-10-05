# Qualification record

Recorded on 2026-10-05. The implementation remains pre-qualification.
Source availability, package builds, execution, numerical checks, convergence
and physical validation are independent gates.

## Actual OpenLB CPU reference

The real OpenLB 1.9.0 driver was compiled with GCC 15.3.0 against revision
`145cd54810b468f4b6fd3ed86b10644264841578`, with the local VTK precision patch.
The test uses a **procedural STL**, independently of FreeCAD. It is evidence
for this bounded CPU formulation, not for CAD integration or a GPU backend.

| Parameter | Explicit fixture value |
|---|---|
| Geometry | 0.02 × 0.01 × 0.01 m channel; STL vertices in mm |
| Boundaries | Periodic x/z; stationary bounce-back y walls |
| Kinematic viscosity | 1e-5 m²/s |
| Acceleration | 0.001 m/s² |
| Density | 1 kg/m³, synthetic material |
| Precision / collision | Float64 D3Q19 forced BGK, relaxation time 0.8 |
| Duration / retained times | 20 s; 0, 10 and 20 s |
| Numerical gate | Relative velocity L2 error ≤0.05 and improvement under refinement |

| Resolution | Fluid cells | Lattice steps | Lattice Mach | Relative velocity L2 error |
|---|---:|---:|---:|---:|
| 8 | 1024 | 1280 | 0.0270632938683 | 0.0111244282661 |
| 16 | 8192 | 5120 | 0.0135316469341 | 0.00278138734196 |

The Python verifier independently decodes zlib-compressed VTK payloads,
checks actual Float64 byte lengths and finite values, follows relative
PVD→VTM→VTI references, and compares the retained velocity to
`u(y)=a*y*(h-y)/(2*nu)`. The driver independently computes the same error
using OpenLB's relative L2 reduction. Material IDs and physical pressure
are retained; pressure has not received an independent numerical benchmark.
Time collections use lattice steps, with explicit requested/observed SI
time mappings in the receipt. Solver convergence remains **not assessed**.

```sh
python3 scripts/verify_openlb_cpu.py \
  --executable /absolute/path/to/harbor-cad-openlb \
  --output /absolute/new/evidence-directory
```

Local authoritative run artifacts:
`/data/scratch/tmp/opencode/harbor-cad-openlb-final/`.
Each resolution directory contains the input plan, procedural STL, process
log, execution/numerical receipt and native VTK files.

Earlier high-drive runs at 0.1 m/s² exceeded the fixed driver's low-Mach
limit. Their velocity agreement is **not qualifying evidence**. The planner
and driver now reject lattice Mach >0.1 before running this formulation;
they preserve the caller's scientific parameters.

### Writer repairs

OpenLB's original `SuperVTMwriter3D<T,T>` labelled arrays Float64 while its
buffer and binary payload remained Float32. The regression verifier failed
on the actual payload length. The recorded patch preserves `OUT_T`, sizes
the zlib buffer with `compressBound`, checks compression failure, rejects
UInt32 payload overflow, and retains coordinate precision.

The original driver also surrounded the periodic box with x/z wall layers,
producing effectively zero through-flow. The driver now uses cell-centred
periodic extents with walls only in y and checks the STL dimensions and
expected fluid-cell count. It rejects other voxel geometries rather than
claiming generic CAD-flow support.

## Real systemd lifecycle

```sh
python3 scripts/verify_systemd.py \
  --executable /absolute/path/to/harbor-cad \
  --output /absolute/new/lifecycle-directory
```

The executed test used the million-point synthetic analytical reference
with an explicit 0.01 tolerance, a 512 MiB profile and a 60 s time limit.
It passed:

- a forced worker SIGKILL followed by worker restart while the independent
  user-service job completed successfully;
- cancellation through the matching persisted service InvocationID;
- effective `MemoryMax=176777216` (the approved plan's peak estimate),
  `TasksMax=128`, `CPUQuotaPerSecUSec=1s`,
  `NoNewPrivileges=yes` and `KillMode=control-group`.

Receipts: `/data/scratch/tmp/opencode/harbor-cad-systemd-frozen-profile/verification.json`.
This establishes the tested single-process reference lifecycle. Native
descendant-tree cancellation, logout/reboot behavior and GPU/importer/JIT
sandbox qualification still require separate execution.

## Worker, transport and artifact checks

- Rust tests cover units/applicability, immutable approvals, durable
  idempotency, restart, traversal/symlink rejection, service-name ownership,
  explicit CPU/B1 planning and physical-role separation.
- Official MCP SDK stdio-client tests exercise the same Rust worker;
  CI rejects native planning and exposes no shell/evaluation tools.
- Export tests reproduced partial final-directory publication and silent
  truncation after 256 artifacts. Exports now stream verified copies into
  private staging, sync them, and publish with no-clobber atomic rename.
  A 302-artifact export retains every registered shard.
- Native output ingestion makes verified copies of closed files, preserves
  nested relative references, and rejects symlinks, special files and
  incomplete outputs. Native arrays remain outside protocol responses.
- Byte-bounded artifact pages traverse 302 long descriptors through the real
  Unix-socket client without response overflow or missing shards. Official
  MCP tests traverse the same cursor API. Whole-bundle export remains complete.
- Admission reserves one plan per state root across worker restarts, counts
  retained artifact bytes, includes native copy staging, and leaves capacity
  waits queued. The CI profile's disk allowance is 8 MiB for multiple retained
  runs; its scientific per-plan allowance remains unchanged.
- Host profiles are atomically bound with submissions and checked by digest.
  A real worker test modifies the source configuration while a job is waiting
  for disk space, then verifies that the admitted job uses and exports its
  original profile. Reusing a key with profile drift is rejected.
- Physical-card reservations now use private per-user anchors across state
  roots rather than separate locks per worker. Tests verify case-normalized
  PCI aliases, shared compute/render/media ownership, contention, release of
  partially acquired sets, and rejection of traversal/symlink anchors. This
  establishes lock behavior, not measured GPU execution or VRAM headroom.
- Approval validation now checks hand-built OpenLB plans against the same
  low-Mach formulation gate as generated plans, requires integral periodic
  extents, rejects SI timestamps collapsing onto one lattice step, and
  requires retention of the final scientific state. Converter arithmetic
  follows the pinned OpenLB constructor and `getLatticeTime` source.
- Adapter process-group tests exercise actual descendants under monitoring
  errors, timeouts, and early leader exits, and verify that unrelated groups
  remain alive. The leader is observed with `waitid(..., WNOWAIT)` and kept
  unreaped until cleanup finishes, preventing recycled-PGID cleanup. Actual
  importer/GPU sandbox and detached-session descendants remain separate gates.
- Failed native attempts snapshot regular outputs and opaque partial files
  with failed-attempt provenance. Unsafe and over-budget entries remain raw
  and are explicitly reported. Tests cover verified copies, unsafe omissions,
  over-budget preservation and terminal-failure export. Raw outputs after forced
  service death are recovered after confirmed service termination.
- Terminal exports include a checksummed `execution.json` with the original
  plan, recorded profile and job state; a failed export is not promoted into
  successful scientific evidence. Quarantined symlinks are counted for disk
  admission without following them or poisoning later jobs.
- Historical plans are archived with their original checked digest and inputs
  even when the current applicability gate rejects relaunch. The exported
  `current_plan_check` reports that rejection; archival does not weaken the
  active submission/launch gate.
- Native runtime paths reject lexical traversal out of `/nix/store` and
  validate their canonical store destination. The SI plan is outside the
  writable native tree and mounted read-only, removing its writable sandbox
  alias. These source-level repairs still require an effective sandbox test.

Local Rust checks used `rustc 1.100.0-nightly (574ff7d98 2026-09-14)` from the
approved Canix environment. The locked CLI package also built with the declared
Rust 1.94.0 toolchain. Python checks used Python 3.13.15 and uv 0.12.5.

Latest local checks passed: 37 Rust integration tests plus five Rust unit tests, Clippy with
`-D warnings`, two official-MCP/Python tests and five render-contract cases, Ruff lint/format checks, and
scoped treefmt. The actual OpenLB CPU and user-manager tests above are
separate opt-in runtime evidence.

Additional native-boundary checks bind imported CAD to a distinct digest-checked
snapshot, reject symlinks/budget overflow without publishing input bytes, and
reject absent or mismatched CPU execution receipts. Receipt descriptors reject
FIFOs, symlinks and oversized records without blocking. Native jobs archive the exact
selected runtime manifest. `scripts/verify_native_cpu.py` prepares the packaged
FreeCAD → CPU OpenLB → portable-bundle qualification through CLI and MCP; its
packaged synthetic-fixture run passed. The CPU runtime does not select
ParaView/media closures.

The CAD-only planner exposes `case plan-cad-inspection` and MCP
`cad_plan_inspection` in the cad/all profiles. It binds an explicit relative
source and lowercase SHA-256, retains the caller's case and uses only the
importer and bundle stages. Hand-built plans receive the same source/digest
checks. CPU protocol tests prove the CI policy rejects native inspection and
that the cad profile cannot submit jobs implicitly.

An actual regression test proved that an environment-only fake `INVOCATION_ID`
could previously execute a service job. Job startup now verifies the live unit's
invocation, main PID and exact cgroup membership before its state transition,
and archives `service-owner.json`. The subsequent real systemd restart and
cancellation test passed against one frozen binary, with the archived owner
matching the user manager. One earlier rerun failed because a concurrent Cargo
build replaced the running development binary; the verifier now copies and
checksums its executable before launching. That failure remains retained and
does not count as passing evidence.

An actual Bubblewrap 0.11.2 probe showed that `/work/plan.json` leaves an empty
mount-point file in the writable output tree. The worker now mounts its trusted
plan at `/plan.json`, outside that tree. A second primitive-level probe passed:
the plan and store were read-only, the host-private marker and user-manager
socket path were hidden, host-loopback TCP was unreachable, GPU nodes were
absent and no mount-point file polluted output. This verifies those tested
primitives, not the full packaged importer/JIT/GPU policy.

The extended real systemd verifier first failed because a forcibly killed
service's raw partial bytes were absent from export. After the fix, worker
restart following owned-service SIGKILL and ordinary cancellation both exported
the exact seeded bytes under `failed-native/` with `native-failure.json` and the
correct failed/cancelled status. The raw fixture is explicitly synthetic opaque
adapter data, not a native solver result. Recovery uses the original
digest-checked plan's output allowance, retains quarantined omissions, and
records recovery errors in the terminal diagnostic. The worker does not attach
to a changed service invocation.

The later packaged native recovery qualifier also passed with actual FreeCAD
and OpenLB at resolution 64. After the initial native channel VTI closed,
owned-service SIGKILL plus worker restart produced a failed job (exit 9), and
ordinary cancellation produced a cancelled job (exit 143). Both bundles have
23 verified records and preserve the exact 61,287-byte initial VTI, including
independently decoded finite Float64 velocity, pressure and material arrays
with shape 130×68×66. The original runtime manifest and live service owner
are retained; no successful OpenLB receipt is present. This qualifies partial
native-field recovery, not solver resume, convergence or a completed solve.

```sh
python3 scripts/verify_native_recovery.py \
  --executable /nix/store/CLI/bin/harbor-cad \
  --runtime /nix/store/RUNTIME-harbor-cad-native-runtime.json \
  --output /absolute/new/native-recovery-directory
```

The verifier freezes its CLI inode, uses a two-GiB RAM/one-GiB disk profile
and one thread, and operates only on its own job units. Exact package,
receipt and field hashes are in [evidence/native-recovery.json](evidence/native-recovery.json).

The hosted CPU workflow is generated from `simit.toml` by Simit revision
`afb7939d925d3e8e9b8507387ada7efad6460df8`. The installed 0.19.0 binary
rejects `[ci].check_command`; an archived clean revision was compiled without
changing the Simit checkout. Generated-file `--check --diff` passes. The
workflow uses commit-pinned actions, a hash-locked uv 0.12.5 bootstrap,
`rust-toolchain.toml`, locked Cargo/uv dependencies and explicit Python
3.13.15. It runs formatters, lints, CPU contract/path/admission/export tests,
and the real official MCP client on public hosted runners with read-only
repository permission. The workflow has not executed remotely because no
push has been performed. Systemd and native/GPU execution remain opt-in local
checks. Native Nix CI must be added after the actual consumer lock/build gates
are available; a passing CPU workflow cannot establish A0 or B1.

`python3 scripts/check_cpu.py` is the shared 20-minute CPU gate. The current
generator retains event-scoped concurrency groups, so push/PR runs are not
coalesced, and it does not expose a total job timeout; these generator gaps
remain recorded upstream requirements. Generation defaults are preserved.

## Current gate status

| Gate | Status |
|---|---|
| A0 | Partial: locked CLI/MCP/native CPU packages and clean/policy/parity checks passed; production importer isolation remains unqualified |
| A1 | Analytical airflow reference plus real low-Mach OpenLB CPU velocity/refinement check; thermal/wetting/FEM references incomplete |
| B1 | Packaged FreeCAD→CPU OpenLB→bundle passed through CLI/MCP; retained CPU fields→EGL surfaces→VAAPI adapter probes passed; required CUDA solver/worker integration unqualified |
| B2 | Renderer/media RAM/CPU/wall/output measured; no qualified numerical GPU filter, per-process VRAM peak or complete topology/ghost-cell round trip |
| C–F | Not implemented |

Read-only inventory exposed AMD devices at `0000:03:00.0` (with a render
node) and `0000:7d:00.0` (without a render alias); no CUDA device was
available. Inventory is not EGL/VAAPI execution evidence. CUDA UUID/minor
correlation and per-device sandbox mounts remain explicit rejection gates.

### Packaged CAD inspection and selected-device media

Five additional Canix gates passed against `96d3942838179fefa664373be7816c6fcfd8c026`:
CLI, MCP, media, Python protocol tests and clean runtime. The packaged inspection
verifier passed real FCStd import through both CLI and MCP, preserving source
bytes and producing 12 checksummed records per CAD-only bundle. Changing the
approved source caused a failed job before importer launch, with no input
snapshot/import receipt and seven checksummed diagnostic records. The full
resolution-8/16 CPU reference also passed again.

The actual `NativeProcess` library with packaged Bubblewrap 0.12.0 stopped a
detached-session child's writer on normal exit, timeout and monitor rejection.
Its scratch harness used dependencies with exact source/checksum parity to
the repository lock. This closes that bounded detached-descendant gate;
worker SIGKILL/systemd cancellation evidence is recorded independently above.

The packaged FFmpeg 9.0.1 adapter encoded three explicitly synthetic 128×128
PNGs with `h264_vaapi` on `0000:03:00.0`, observed as Radeon RX 7900 XTX through
Mesa Gallium 26.2.1/radeonsi. Actual CPU decoding verified three yuv420p frames,
dimensions and presentation timestamps 0, 0.041667 and 0.083333 s at 24 fps.
Observed configuration enables GPL/version3 and disables nonfree. This qualifies
only the selected media adapter probe, not EGL, physical-time labels or B1.

Initial probing exposed a sandbox defect: libdrm 2.4.134's `drmNodeIsDRM`
requires selected-device sysfs metadata even with an open render FD. Its official
archive was verified against locked Nixpkgs' SHA-256. The production Rust binding
now correlates the live character-device major/minor, DRM sysfs entry and exact
PCI device, mounting six PCI metadata attributes and the selected render sysfs
directory read-only. PCI config/resources, other cards and the rest of sysfs
stay hidden. Actual production-library encoding and mount-isolation probes
passed under a bounded systemd service. A separate 64×64 fixture correctly
failed the driver's reported 128×128 hardware minimum; the qualifier now uses
an explicit 128×128 fixture and does not resize user frames.

Exact build outputs, qualification hashes and scoped source evidence are in
[evidence/inspection-media.json](evidence/inspection-media.json).

### EGL display identity and calibration

The production EGL helpers passed an offscreen OpenGL calibration under the
production DRM sandbox and owned native-process library. The current initialized
EGL display's `EGL_DEVICE_EXT` resolved to `/dev/dri/renderD128`, matching selected
PCI `0000:03:00.0`. Observed EGL 1.5/OpenGL 4.6 used the Radeon RX 7900 XTX and
Mesa 26.2.1/radeonsi. Every 64×64 framebuffer readback pixel matched the expected
RGBA calibration value, with zero GL error and no software renderer.

The pinned VTK source shows that `DeviceIndex` represents requested selection
and that default-display fallback exists. The adapter now checks the live
display's actual device before issuing a success receipt. The Khronos extension
defines `EGLAttrib` as `intptr_t`; the bridge uses the matching ctypes width.
Bounded enumeration rejects missing, changed and ambiguous inventory. Five CPU
regression cases cover actual-device rejection and authoritative channel time
collection selection, including rejection of a symlink/material-only collection.
The driver emits both `channel.pvd` and `geometry.pvd`; only the channel collection
uses the velocity/pressure observation clock. Unsupported presentation fields
are rejected explicitly.

The first headless ParaView package failed CMake's relative-install-destination
check. The committed fix uses the pinned Nixpkgs recipe's relative GNUInstallDirs;
later ordinary Canix gates passed against `9511dba` and `dd3ccbc`. The preceding
retry realized ParaView but its final Canix receipt rejected source drift; that
attempt is not counted as a passed guarded gate. The archive hash, runtime
library and initial calibration receipt are in [evidence/egl-device.json](evidence/egl-device.json).

### Retained-field EGL surfaces and VAAPI video

The packaged renderer and media adapter passed with resolution-16 CPU OpenLB
fields from the checksummed native bundle. All native files and science
parameters remained unchanged. The recipe explicitly chooses a presentation
camera `[0.04, 0.025, 0.03]` and fixed velocity scale 0..0.0015 m/s. Actual EGL
context identity matched the selected RX 7900 XTX/renderD128; no software
renderer was accepted. The three 640×480 surface frames correspond to lattice
steps 0/2560/5120 and observed physical times 0/10/20 s.

Pixel inspection of the first successful renderer probe showed only an outline.
A native assertion then proved that ParaView's first render also changed the
requested camera automatically. The adapter now explicitly renders velocity
magnitude surfaces, disables that reset and automatic color-range growth,
and verifies camera/range/representation after every screenshot. The receipt
retains each frame's native step, observed physical time and annotation.

The selected `h264_vaapi` encoder produced three yuv420p frames at 24 fps;
actual CPU decoding verified dimensions, count and presentation timestamps
0/0.041667/0.083333 s. A separate device-free sandbox decoded the video into
PNGs. Every decoded image visibly preserves its 0/10/20 s label, units and
fixed scale; the zero-time surface and later velocity colors are distinct.
The final run's video is byte-identical to the independently inspected video.
The playback clock does not represent the solver's physical-time spacing.

The qualifier uses the production DRM binding, native-process containment and
shared physical-card reservation. Measured whole-service RAM peaks were
189,026,304 bytes for rendering and 74,137,600 bytes for media; elapsed times
were about 1.266 s and 0.414 s, including stage startup and native cleanup.
These are single measurements, not equal-accuracy performance comparisons.
Per-process VRAM accounting and headroom remain unqualified. A negative probe
requesting unretained time 5 s failed before producing any frame or success
receipt, without altering the fields or relaunching a solver.

```sh
cargo build --locked --example native_stage_probe
python3 scripts/verify_egl_fields.py \
  --probe "$PWD/target/debug/examples/native_stage_probe" \
  --runtime /nix/store/RUNTIME-harbor-cad-native-runtime.json \
  --render /nix/store/RENDER/bin/harbor-cad-render \
  --media /nix/store/MEDIA/bin/harbor-cad-video \
  --bundle /absolute/native-cpu-bundle-16 \
  --pci 0000:03:00.0 --output /absolute/new/egl-field-probe
```

The script records receipts and hashes; independent image inspection is required
to qualify label pixels for a new run. Exact package/code/video/decode hashes,
resources and rejection evidence are in [evidence/egl-fields.json](evidence/egl-fields.json).
An additional CPU-only probe used the same packaged ParaView PVD/VTM reader
at every retained step, comparing its actual ImageData arrays with independent
zlib decoding. All velocity, pressure and material Float64 payloads matched
bit-for-bit, with their component counts, point association, complete extents,
SI origin and spacing. This 34×20×18-point fixture has no cell or ghost arrays;
the result does not qualify other topologies or partition/ghost handling.
The rebuilt hardware-independent Nix Python check also passed all seven
protocol/render-contract cases under ordinary Canix admission against `3abc667`.
This closes the bounded selected-device field-render/media adapter probe,
while required GPU OpenLB, worker B1, numerical GPU filters and physical
validation retain their separate gates.

## Canix package evaluation and realization

All eight selected package/check targets passed through ordinary Canix admission
against `bffb87bf2db916fcffc41bc1c838915b5bb2969f`: CLI, MCP, CPU OpenLB, CPU
runtime, clean-runtime, Python protocol tests, effective unfree/Fleetix policy,
and the actual procedural-STL OpenLB numerical reference. Builds used one job,
two cores and `--no-push`; no host activation occurred. The CLI used Rust 1.94.0;
the packaged solver reported GCC 16.2.0. Exact derivations, retained output
paths, logs and hashes are in [evidence/packaged-cpu.json](evidence/packaged-cpu.json).

`scripts/verify_native_cpu.py` then exercised packaged FreeCAD 1.1.4 and OpenLB
inside Bubblewrap 0.12.0 under tracked systemd jobs. Resolution 8 used CLI and
resolution 16 used the official MCP stdio client, both against the same worker.
Every exported record, runtime manifest and service owner was verified; retained
Float64 velocity/pressure/material fields preserved the requested 0/10/20 s
mapping. Each bundle contains 28 checksummed records. The velocity errors match
the independent procedural-STL reference above and improve about fourfold.
This is synthetic fixture/CPU integration evidence; it does not establish
pressure accuracy, convergence, GPU execution or physical validation.

`canix repo eval --wait-seconds 300 --directory
/data/nvme0/can/canix/projects/repos/owned/harbor-cad
'git+file:///data/nvme0/can/canix/projects/repos/owned/harbor-cad#packages.x86_64-linux.default.drvPath'`
passed against commit `2e52eb0699f471ec0f00f1f2a7013dc7444ad1a1`, resolving
`/nix/store/h1g62w3d4fssnjqnh4jr5x9hzm448gc8-harbor-cad-0.1.0.drv`.
This is evaluation evidence only. The read-only evaluation resolved inputs
but did not write `flake.lock` or realize the CLI. The subsequent correctly
routed scoped Canix update succeeded and created `flake.lock`; all 11 root
revision pins were checked against the dependency manifest, and the lock records
their generated NAR hashes.

The first package-build sequence did not enter the evaluation phase: its
300-second ordinary-admission window ended behind PID `4022767`, running
`canix cache binary build .#roborev-aarch64 --no-push --max-jobs 1 --cores 2`.
No Harbor-CAD package was realized by that attempt. The exact failed receipt
is retained alongside the scoped source/runtime evidence.

CLI, MCP, CPU OpenLB, CPU runtime and policy/clean/Python check derivations also
evaluated successfully. Subsequent lock/build attempts encountered the shared
evaluation lease. The installed update/build commands have no evaluation-wait
option, so scoped updates use bounded ordinary-admission retries.

The Home Manager launcher overwrites `CANIX_FLAKE_ROOT` with the parent Canix
checkout; a preceding managed-input rejection came from that routing. Using the
same installed package's `bin/canix` preserves the explicitly selected root and
normal Canix guards. It then reported Harbor-CAD's missing `cachePinMeta`.
The consumer now declares supported schema 4 with no cache-managed inputs; all
its dependencies remain immutable pins. No upstream/host change, raw Nix
override, inherited evaluation lease, concurrent bypass or upload was used.
Lease-release notices allowed another lane's normal workspace admission to
complete before the next Harbor-CAD evaluation.

Commands use the installed package CLI for updates and target-checkout cwd for
builds. From an approved environment in the Harbor-CAD checkout:

```sh
CANIX_PROJECT_CLI=/nix/store/77dc01dssq4ss86xw8d2z5fxa39l3q3r-canix-admin-0.1.0/bin/canix
CANIX_FLAKE_ROOT="$PWD" "$CANIX_PROJECT_CLI" repo update flake --input nixpkgs
canix cache binary build .#default --no-push --max-jobs 1 --cores 2
canix cache binary build .#mcp --no-push --max-jobs 1 --cores 2
canix cache binary build .#openlb-cpu --no-push --max-jobs 1 --cores 2
canix cache binary build .#checks.x86_64-linux.clean-runtime \
  --include-tests --no-push --max-jobs 1 --cores 2
```

The package sets explicitly override Harbor-Py's permissive unfree default.
The optional CUDA set uses an enumerated predicate. Effective Nix policy
checks passed for both effective package sets and Fleetix source/digest parity.
Active-job closure retention, aggregate multi-worker admission, GPU VRAM budgets,
effective importer/JIT sandbox profiles, and engineering physical inputs
remain qualification work.
