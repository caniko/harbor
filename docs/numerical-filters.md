# Source-bound numerical compute filtering

`harbor-cad filter REQUEST.json` and MCP `filter_plan` in `simulation`/`all`
produce an independent version-4 approval. Submit it with the ordinary approved
`job submit`/`job_submit` operation. Planning returns descriptors; it transports
no scientific arrays and launches no solver.

The initial request is intentionally bounded:

```json
{
  "source_job": "COMPLETED_REGISTERED_SOLVER_JOB_UUID",
  "filter": { "time_s": 20, "field": "velocity" },
  "compute": {
    "role": "compute",
    "backend": "hip",
    "pci": "0000:03:00.0",
    "backend_uuid": "EXACT_CURRENT_HIP_UUID"
  }
}
```

Supported field choices are `velocity` and `pressure`, from one retained time
and one registered Float64 image-data shard. The source is a completed authorized
OpenLB job with immutable scientific fields and units. A filter job binds the
source plan, execution, authorization, snapshot and bytes. It receives distinct
verified retained-source inodes before acknowledgment, with the same filesystem
accounting and orphan recovery used by presentation. It has a separate compute
execution binding under `harbor-cad-filter-hip-single-kfd-v1`.

The DAG is `numerical_filter → bundle`; it contains no CAD, solver, EGL or media
stage. Explicit required HIP execution shares the canonical RAM/filesystem and
physical-card admission authority. The worker resolves the selected single-KFD
identity, applies its operation-specific selected-node mounts, and exposes the
copied fields read-only. Cancellation and restart use the existing owned service
tree and durable reservation rules.

`runtime-filter-hip` identifies the separate native `filter-hip` package. It
contains no renderer, encoder or OpenLB executable. Package declarations and CPU
contract tests are not native/hardware qualification; the compatibility record
and actual qualification report must be checked for the exact runtime.

## Outputs and checks

The first adapter bounds input XML to 64 MiB, one million image points and
256 MiB of output. Worker-approved conservative stage minima are 1 GiB RAM and
1 GiB VRAM, with independent driver/card headroom controlled by authority.
These are conservative estimates; measured RSS and tracked Kokkos allocations
are checked against them after execution. Whole-card VRAM peak measurement is
not inferred from library allocation callbacks.

Stage-local output is committed under `stages/filter/`: Float64 `gradient.vti`,
the native receipt, log and a trusted `field-description.json`. That descriptor
binds original science and physical time to the new numerical execution/output
hash, preserves original field units, and adds `gradient` in `1/s` for velocity
or `Pa/m` for pressure. Parent scientific fields are neither rewritten nor
relabelled as a new physical solve.

The worker checks native HIP PCI/UUID, architecture, compiler/runtime/driver
identity, exact VTK/Viskores/Kokkos source revisions, actual HIP dispatch and
completion evidence, original science/source checksums, output bytes and
Float64/topology round-trip evidence. CPU fallback cannot satisfy this approval.
An explicit native CPU-reference command exists for the opt-in qualifier;
it is not a required-GPU job backend.

Numerical round-trip verification does not prove gradient accuracy, convergence
or physical validity. Linear/quadratic references, CPU/HIP comparisons,
instruction-level traces, filter sandbox/lifecycle tests and measured whole-card
memory remain independent B2 acceptance work. Ghost arrays, broad topology,
cell-field averaging and general filter graphs remain outside this allowlist.
