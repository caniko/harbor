# Acceptance and execution ledger

Baseline: `d9aff76f71f7bd6b06ec54d1fad277271d1a0516`, branch
`implementation/local-worker`. This ledger records scoped evidence and remaining
acceptance work against [the implementation specification](implementation-spec.md).
Recorded qualification does not qualify a changed runtime or another host.

## Baseline verification — 2026-10-06

- `python3 scripts/check_cpu.py`: passed before this slice and after the
  source-bound presentation implementation; Rust tests, Clippy `-D warnings`,
  locked build, formatting, Ruff and 35 Python cases.
- `treefmt -C <repository> --config-file <repository>/treefmt.toml`: passed.
- Compatible pinned Simit `init ci --check --diff`: passed. Hosted CI remains
  unexecuted because work is local.
- `flake.lock` SHA-256:
  `c82b720484076521ba2a59ca4bf891873b69971a0c9766bd7f58c181d403a807`.
- `Cargo.lock` SHA-256:
  `086eb4d8227bbdaf2e376c1eba42ec43cba8bdb09d5805b8a17a2a8fbce68ae6`.
- `uv.lock` SHA-256:
  `b1d44d4f31b32796c5f759964ff934aa9933f6df769f6888ed17982ab7fe0f25`.

## Requirement map

Post-P1b local verification also passes the complete CPU gate with 39 Python
cases, strict version-3 frame approvals, copied-frame/time/dimension checks,
source-loss orphan preservation and concurrent admission bootstrap. Bundled
SQLite is upgraded to 3.53.2 through rusqlite 0.40.2 to include the upstream
WAL-reset fix; updated `Cargo.lock` SHA-256 is
`cebcbe7034f99fd0bbd82cbb7cda0df6630e01dff47603abaf8fc3ca010148f8`.
Generated CI drift check passes. Packaged candidate evaluation reached the
600-second wait limit while another Canix operation owned the host guard; no
lease bypass or resource override was used.

Subsequent guarded evaluation and one-job/two-core realization completed for
`6ec1289df74593d682c71690f71b9f53136a863f`. The packaged P1 qualifier passed
CLI/MCP retained-field rendering and independent registered-frame VAAPI video,
including direct source/frame mutation before/after acknowledgment and
worker restart/retry with unchanged service invocation. Cancellation preserved
science, all five new service trees closed, and runtime roots/reservations
released. See [scoped P1 evidence](evidence/standalone-presentation.json).
Version-4 source-bound HIP gradient contracts/CLI/MCP pass the CPU gate; their
separate native build and hardware qualification remain in progress.

Job-scoped `qualify --job` and results-profile MCP `qualification_report` now
inspect checksummed historical receipts and immutable execution/device/source
identities. They distinguish recorded execution from reported numerical checks
and leave current-runtime qualification/physical validation unpromoted.

An independent locked Gmsh/CalculiX CPU candidate now implements synthetic
steady conduction/Fourier flux and free-expansion reference decks, semantic
planar face predicates, positive-Jacobian/volume correspondence and complete
native field parsing. Its source/manual APIs are inspected and parser/selector
rejection tests pass. The exact native references and version-5 CLI/MCP worker
integration now pass their separate gates; see
[FEM reference scope](fem-references.md) and [exact evidence](evidence/fem-cpu.json).
Static solver parameters retain no invented physical-time mapping.

The independent version-6 prescribed transient thermal path is implemented:
CLI/MCP planning binds explicit SI histories/material domains/provenance,
native solver substeps and independent physical output times. Its CPU-only
worker policy mounts an operation-specific closure and requires conservative
authority-backed admission. The complete CPU gate now passes with 66 Python
cases, including legacy/schema/gate rejection and imported geometry/origin
contracts. `thermal-native-5` passed eight native solves, nine pre-output
rejections and separate spatial/time assessments at the recorded exact package;
the first worker attempt rejected a CPython cache entry before solving. The
bytecode-repaired native package passed `thermal-native-6`; a one-CPU timeout
in `thermal-worker-2` led to a fresh explicit two-CPU `thermal-worker-3`, which
passed CLI/MCP, raw-field/energy checks, exports, restart/idempotency, forced
complete-tree death, cancellation and final release. Exact evidence is in
[thermal evidence](evidence/thermal-cpu.json).

The updated complete CPU gate passes 103 Rust tests and 44 Python cases,
Clippy with warnings denied, locked build, Treefmt and Ruff. New systemd jobs
verify effective kernel controls before native launch and retain aggregate
process-tree RAM peaks/CPU usage through orderly completion; abrupt kills have
explicitly absent final peak records. Packaged effective-control and B2 native
qualification are still pending.

The guarded filter build from `06b018a8aaae754353acbd3d11a32bb3cf2e4877`
successfully compiled the pinned Kokkos, VTK/Viskores and native HIP adapter.
This establishes a build, not native numerical/device qualification. The
subsequent scalar-label/direction guards and latest runner must be realized
before their exact-package qualification.

CAD-facing `cad inspect|regions|export` and MCP `cad_submit`/`cad_regions` now
use the same approved worker jobs. Region views are bounded/checksummed and bind
the succeeded native CAD stage, original science/execution and placement units.
The CAD profile can submit its own inspection DAG and read durable status/logs;
foreign plans and receipts reject. Ordinal face identities, controlled variants
and imported-CAD/Gmsh correspondence remain pending.

Exact revision `358fd57f7c0a9933aafe64db63af69de7b7c91ce` passes the guarded
native B2 analytical/CPU-HIP gate and the separate packaged CLI/MCP worker gate.
Both scalar/vector linear gradients, quadratic interior gradients, boundary
CPU parity, retained velocity/pressure and 13 negative cases pass `1e-10`.
Worker source mutations, restart/retry with unchanged invocation, selected
single-KFD binding, effective controls/aggregate peaks, forced death and
cancellation pass. Original source hashes are unchanged; reservations and
runtime roots end at zero. See [exact scoped evidence](evidence/numerical-filter-hip.json).

| Specification | Implementation / verification owner | Evidence / remaining acceptance |
|---|---|---|
| §1 local CLI, worker, MCP, file/native boundaries | `src/main.rs`, `src/worker.rs`, `python/harbor_cad_mcp`, `adapters/` | CLI/MCP B1 recorded; remaining recipes below |
| §1 selected native stacks; no extra mandatory framework | `nix/native.nix`, `nix/openlb.nix`, `nix/fem.nix` | FreeCAD/OpenLB/ParaView/FFmpeg and isolated Gmsh/CalculiX packaged and scoped native gates passed; spectral integration pending |
| §2 Harbor and Fleetix merged PR #3 | `flake.nix`, `Cargo.toml`, `build.rs` | pinned signatures/contracts; source and digest drift tests |
| §2 packages, modules, clean-runtime discovery | `flake.nix`, `nix/modules.nix`, `tests/worker.rs` | lightweight CLI/MCP and native packages built; remaining backend packages pending |
| §2 independent interpreters and immutable ABIs | `nix/native.nix`, `docs/dependency-manifest.json` | importer/MCP/EGL and isolated Gmsh/CalculiX worker executed; spectral ABI sets pending |
| §2 locks, native patches, GPU architecture | lock files, `nix/patches/`, `nix/openlb.nix` | immutable pins, `gfx1100`, isolated CPU FEM and compatible HIP filter verified; Dr.Jit set pending |
| §2 unfree, driver boundary, caches and closure retention | `flake.nix`, `src/retention.rs` | scoped policy, no host activation; active roots/recovery qualified |
| §3 compute/render/media identities | `src/devices.rs`, `src/authority.rs` | exact HIP PCI/UUID and initialized EGL/VAAPI identities recorded |
| §3 shared admission, headroom and required execution | `src/admission.rs`, `src/resources.rs`, `src/estimates.rs` | RAM/disk/card contention and death retention qualified; FEM fill-in estimates pending |
| §3 measurements, JIT and partitions | `scripts/verify_openlb_hip.py`, native receipts | scoped kernel evidence; equal-accuracy timings, VRAM peaks and multi-GPU qualification pending |
| §4 schemas and Python parity | `src/contracts.rs`, `python/tests/test_protocol.py` | strict v1–v9 plans and original approval compatibility tested; broader recipe-specific contracts pending |
| §4 prepare/run/inspect/capabilities, verified resume | fixed adapters and receipts | native execution available; explicit capability keys and supported resume pending |
| §4 units, applicability, identities, unknowns | `src/science.rs`, `src/contracts.rs`, `src/materials.rs`, `src/recipes.rs` | SI/identity/rejection, thermal property domains and missing-input preservation tested; native contact/optical execution pending |
| §4 controlled CAD, tags, meshes, variants | `adapters/freecad_bridge.py`, `src/storage.rs` | source snapshot/import isolation qualified; parameter copies and geometric region selection pending |
| §4 allocated solver resources, unresolved paths | `src/estimates.rs`, `adapters/openlb_channel.cpp` | channel allocation/Mach/model gates; other formulations and local-gap resolution gates pending |
| §5 cold start and expansion/contact | recipe contracts and native FEM adapters | synthetic transient prescribed histories/heater energy and static free expansion pass native and CLI/MCP gates; contact data and conservative temperature transfer pending |
| §5 airflow/wetting/snow/freezing | OpenLB recipe drivers | synthetic single-phase flow and planar wetting native campaign qualified at recorded identities; wetting worker launch repair in requalification; prescribed coverage and freezing conservation/refinement pending |
| §5 solar/UV, angular inputs and dose | atmospheric/spectral adapters | orientation/occlusion/reflection/unit/temporal tests and GPU transport pending |
| §5 typed one-way transfers and convection | `TransferSpec` | typed conservative transfers pending; velocity-to-convection inference rejected by scope |
| §5 moisture-risk entry | `src/moisture_results.rs`, thermal source fields | all six native planar thermal surfaces and explicit screening/missing/inapplicable branches pass packaged CLI/MCP moisture-results-1; combined recipe assembly pending |
| §6 durable lifecycle/idempotency/services | `src/lifecycle.rs`, `src/worker.rs`, `src/storage.rs` | restart/disconnect/owned tree/cancel/partial output qualified; logout/reboot/resume pending |
| §6 patched importer and operation policies | `src/sandbox.rs`, `adapters/import_policy.py`, device sandbox | importer and selected single-KFD HIP scope qualified; broader solver/JIT/filter policies pending |
| §6 typed input, argument arrays and credential denial | worker operations, bounded protocol, native sandbox | unknown/path/symlink/session/network canaries tested |
| §7 independent observations and congestion | `ObservationPlan`, native channel output | fixed retained times and fail-on-budget; bounded configurable probes/reductions pending |
| §7 arrays and format round trips | `src/fields.rs`, retained-field verifiers | Float64 image graph preserved; broad topology/ghost/node-cell round trips pending |
| §7 atomic committed data/exports and sole-source safety | `src/storage.rs`, `src/presentation.rs` | distinct verified source copies, staging intents, atomic bundle/checksum tests |
| §7 compute filters vs presentation | `adapters/paraview_bridge.py`, `adapters/video_bridge.py`, `adapters/filters/` | independent EGL/VAAPI and scoped HIP numerical-gradient native/worker gates recorded; broader filters pending |
| §7 portable provenance and retained re-render | immutable snapshot, source-bound v2/v3 plans | packaged standalone CLI/MCP rendering and independent video qualified at the recorded revision |
| §7 Catalyst/Conduit | optional optimization | deferred until measured baseline justifies it |
| §8 CLI/MCP product surfaces | `src/main.rs`, official MCP profiles | job/results/render/video/filter and CAD regions/export/mesh/imported-FEM surfaces implemented and scoped worker gates passed; v5/v8 exact native sample/signed same-mesh compare pass packaged results-3 CLI/MCP gate (eight fields, 40 rejections); CAD variants pending |
| §9 CI and negative tests | `scripts/check_cpu.py`, generated workflow, tests | local gate green; hardware gates remain opt-in and scoped |

## Ordered runnable slices

| Slice | Acceptance | State |
|---|---|---|
| P0 | baseline locks/tests/generated-workflow drift; requirement/prerequisite ledger | complete |
| P1a | explicit presentation approval; atomic retained-source copies; render/optional video CLI/MCP | complete at the recorded package/hardware scope; packaged mutation/lifecycle/exports pass |
| P1b | independent registered-frame video plans, lifecycle and source mutation rejection | complete at the recorded package/hardware scope; CLI/MCP frame mutation, restart/retry and release pass |
| P2 | versioned recipe inputs, stage-local artifacts, conservative typed transfers and native policies | transfer/unit/material/cold-input foundation and independent v5 static FEM implemented; exact CLI/MCP native mesh/fields and operation-specific isolation qualified; closed BREP/imported-box correspondence and origin-aware FEM native gates pass; v7 mesh worker passes at build 22; v8 imported static FEM CLI/MCP/exports/lifecycle passes with matching native prerequisite at build 23; broader geometries/recipes remain pending |
| P3 / B2 | exact compatible HIP numerical-filter stack, observations, RAM/VRAM telemetry and format checks | scoped v4 native/CLI/MCP analytical, CPU-HIP, byte/topology, source/lifecycle and aggregate RAM/CPU gates pass at the recorded revision; instruction trace, whole-card VRAM and broader filters pending |
| P4 / A1 | controlled CAD/regions/Gmsh; thermal, FEM, wetting and flow reference gates | synthetic steady conduction/free expansion at 2/4/8 and CLI/MCP lifecycle pass; imported origin/translated BREP correspondence and static FEM pass native/worker gates; airflow separately scoped; synthetic planar wetting-native-7 passes six solves, twelve rejections and two decreasing-error sequences; worker launch repair requalification pending |
| P5 / C | native thermal/contact/moisture/coupling slice | independent synthetic transient numerical/spatial/time and CLI/MCP worker/export/lifecycle gates passed; exact thermal queries and native surface moisture CLI/MCP gates passed; planar contact feasibility diagnostic runs native CPU solver; packaged contact/coupling pending |
| P6 / D | native local water, prescribed snow and retained-water freezing | pending |
| P7 / E | native atmosphere/spectral irradiance/dose slice | pending |
| P8 / F | bounded studies, retention/recovery/resume, measurements and product closure | pending |

The next results slice adds strict registered-v6 `results sample-thermal` and
`compare-thermal` plus matching results-profile MCP tools. Complete JSON histories
are independently checked against their authoritative DAT counterparts; the
complete native schedule/node coverage is separately verified. Only declared
field-observation times authorize sampling. Native time-serialization
error is exposed separately from exact requested times; signed comparisons keep
both physical states. Treefmt, full CPU checks (81 Python cases) and generated
Simit CI drift pass. The first development diagnostic exposed the differing
JSON observation/DAT energy schedules and remains retained as failed evidence.
The corrected diagnostic matches both qualified thermal jobs at 10/120 s and
their signed differences, including generated report-schema checks. Build 32
predates that correction. Static regression `results-4` passes eight field checks
and 40 rejections. Corrected build 33 passes `thermal-results-2` with eight field
checks and 52 rejections; build 34 also repeats the complete thermal gate.

Source-bound `results moisture` and matching results-profile MCP now reduce
complete geometrically verified native thermal box surfaces at an exact retained
time. Air inputs, missing data and justified inapplicability remain explicit;
subzero surfaces retain the separate unsupported ice/frost status. The full CPU
gate (82 Python cases), Treefmt and Simit drift pass. A retained development
diagnostic checks both thermal jobs, native surface minima and all three
assessment branches. Build 34 passes `moisture-results-1` with 44 thermal/surface
checks and 62 rejections, including all six complete surfaces. All query gates
leave the original closed source trees and copied database records unchanged.
Exact identity/report hashes are in [result evidence](evidence/thermal-results-cpu.json).

Native `wetting-native-7` passes at build 31 with unchanged scientific gates and
explicit equal-duration refinements. `wetting-worker-1` fails before solve due
to its preopened stage log. Repair `8ee49c2` passes the full CPU gate (86 Python
cases); build 35 and fresh native/worker qualification are pending. Prior failed
native campaigns and the failed worker attempt remain retained independently.

## External qualification prerequisites

- Multi-GPU KFD exclusion needs multiple supported live devices and matched
  running-kernel source evidence. The observed host exposes one live AMD KFD GPU.
- Hybrid CUDA FEM and CUDA/OptiX spectral transport need suitable NVIDIA hardware.
  CUDA remains best-effort; available AMD work continues independently.
- The compatible VTK/Viskores/Kokkos HIP filter passes scoped build, dispatch,
  numerical/roundtrip, worker and resource gates; instruction traces, whole-card
  VRAM and broader operations remain separate qualification work.
- Real environmental claims need geometry, material/contact/wetting/optical data,
  operating histories, acceptance limits and prototype evidence. Synthetic native
  reference implementation can progress; missing physical inputs stay explicit.
- Paperclip revised-attempt-2 audit remains independent. Its recorded exit 1 and
  failed tests/build do not establish full application qualification.
