# GPU backend decision

## Decision (2026-10-05)

Prioritize **HIP/ROCm for OpenLB on AMD**, with **CUDA best-effort**. Keep the
independent **ParaView EGL** and **FFmpeg VAAPI** stages. Vulkan is a useful
cross-vendor compute API, but is not an implemented OpenLB backend at our pin.
This decision follows the user's revised priority and upstream source inspection;
it is not a claim that HIP has beaten Vulkan in an equal-accuracy benchmark.

## Evidence

| Question | Finding and source |
|---|---|
| Can the current solver use AMD GPUs? | OpenLB `145cd54810b468f4b6fd3ed86b10644264841578` contains `GPU_HIP`, collision/streaming, communication and device memory implementations. See [`src/core/platform/platform.h`](https://gitlab.com/openlb/release/-/blob/145cd54810b468f4b6fd3ed86b10644264841578/src/core/platform/platform.h), [`src/core/platform/gpu/hip`](https://gitlab.com/openlb/release/-/tree/145cd54810b468f4b6fd3ed86b10644264841578/src/core/platform/gpu/hip), [`rules.mk`](https://gitlab.com/openlb/release/-/blob/145cd54810b468f4b6fd3ed86b10644264841578/rules.mk) and [`config/gpu_only_amd.mk`](https://gitlab.com/openlb/release/-/blob/145cd54810b468f4b6fd3ed86b10644264841578/config/gpu_only_amd.mk). |
| How mature is this backend? | The [OpenLB 1.9 release announcement](https://www.openlb.net/news/openlb-release-1-9-available-for-download/) calls AMD support preliminary, with upstream tests on MI300A and RX 7800 XT. Those tests do not qualify our Float64 forced-channel driver, card or sandbox. |
| Does the source support Vulkan? | `Platform` lists CPU_SISD, CPU_SIMD, GPU_CUDA and GPU_HIP. Inspection of the pinned source and build rules found no Vulkan platform or build path. Adding one would require a new numerical implementation and its qualification, rather than a compiler switch. |
| Does Vulkan run this science on any GPU? | Khronos documents [`shaderFloat64`](https://docs.vulkan.org/refpages/latest/refpages/source/VkPhysicalDeviceFeatures.html) as a queried feature. A Vulkan-capable card does not by itself guarantee the Float64 shaders required here. Feature support, numerical behavior, device identity and actual kernels still need testing. |
| Is Vulkan almost as fast as ROCm? | No same-model, same-card, same-precision Vulkan/HIP comparison was established for this OpenLB pipeline. Performance depends on implementation, memory layout, synchronization, transfers and FP64 throughput. Keep this an unmeasured hypothesis; do not use LLM-inference comparisons as CFD evidence. |
| Can scientific filters reuse ROCm? | Viskores source snapshot [`6fddd7edee9aea5f0388672b7f817960dc3429fb`](https://github.com/Viskores/viskores/blob/6fddd7edee9aea5f0388672b7f817960dc3429fb/CMake/ViskoresDeviceAdapters.cmake) supports HIP through **Kokkos**, with a separately selected HIP toolchain. This is research evidence, not a new dependency pin or qualification of the bundled ParaView filter path. |
| Does AMD list the card family? | AMD's [ROCm 7.2.1 Radeon Linux matrix](https://rocm.docs.amd.com/projects/radeon-ryzen/en/latest/docs/compatibility/compatibilityrad/native_linux/native_linux_compatibility.html) lists RX 7900 XTX/XT/GRE. Its OS list is Ubuntu/RHEL, not NixOS; hardware listing and upstream NixOS tests do not replace testing our pinned Nixpkgs ROCm stack. |

The upstream AMD example has an inconsistent comment/architecture (`RX 7800 XT`
but `gfx942`) and uses Float32. Our package must set an explicit architecture and
Float64; it must not copy those defaults or upstream `HIP_ARCH=native` behavior.

## Host observations

Read-only PCI/KFD inspection on 2026-10-05 established:

- `0000:03:00.0`: AMD Navi 31, PCI device `1002:744c`, bound to `amdgpu`,
  25,753,026,560 bytes reported total VRAM. The PCI name covers multiple SKUs;
  it does not uniquely identify the retail model.
- KFD node 1: `domain=0`, `location_id=768`, `gfx_target_version=110000`,
  `drm_render_minor=128`. These correlate to PCI `0000:03:00.0`, `gfx1100`,
  and `/dev/dri/renderD128`. `/dev/kfd` exists.
- `0000:7d:00.0`: integrated AMD GPU bound to `vfio-pci`; it is not an available
  AMD compute/render candidate in this host configuration.
- `hipcc`, `rocminfo`, `vulkaninfo`, `amd-smi` and `rocm-smi` were absent from
  the inspected PATH. Package-local tools must establish actual runtime support.

KFD is a shared device. Device visibility variables alone are not isolation.
Production admission needs verified PCI/UUID/KFD/render-node correlation and a
tested selected-device mount policy before HIP worker execution is enabled.

## Implementation and acceptance

1. Build the existing OpenLB HIP backend against the locked Nixpkgs ROCm stack,
   targeting `gfx1100` explicitly and retaining Float64/precision patches.
2. Require an explicit HIP PCI + UUID selection. Reject stale UUIDs, wrong
   backend, ambiguous devices, unsupported architectures and CPU block fallback.
   Record runtime/compiler/device identities and kernel completion.
3. Verify the synthetic periodic forced channel against the independent
   analytical reference and the CPU fields at the same resolutions/times.
4. Qualify KFD isolation, resource admission and lifecycle before promoting this
   into durable CLI/MCP B1. Preserve separate build/runtime/numerical/security
   and physical-validation statuses.
5. Advance HIP/Kokkos numerical filters only against an exact compatible
   ParaView/VTK/Viskores set. EGL rendering and VAAPI media retain their own
   selected devices and receipts.

Version-1 approvals retain their serialization. B1 planning now accepts explicit
`hip` or `cuda` compute selections; `rocm` is not a backend alias and `vulkan`
cannot silently select another solver. Existing historical execution bindings
continue to name their original packages and policy.
