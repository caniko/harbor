# HIP scientific-filter compatibility investigation

Inspected upstream sources on 2026-10-06. This identifies a concrete candidate
build path; it does not establish build or runtime qualification.

## Exact compatible source set

- ParaView 6.0.1's annotated tag resolves to commit
  `bdd52330012d733d3d6d85702a0498243af46272`.
- That commit's VTK gitlink is
  `7c0494a68bff379d32d6b1fbaa3d10d27a73af54`.
- Its [Viskores subtree update script](https://github.com/Kitware/VTK/blob/7c0494a68bff379d32d6b1fbaa3d10d27a73af54/ThirdParty/viskores/update.sh)
  identifies `521f3b72aabe0bf37e9972975700df27adbbae71`, just past
  Viskores 1.0.0. This is more specific than the earlier unrelated Viskores
  research snapshot.
- Existing Nix packaging uses ParaView 6.0.1's full source archive, hash
  `sha256-XlasevXpJbPP0/q4JHCTPLq8fo/ah+FK9k+ZXWBk6wY=`. Verify these
  relevant bundled files against the archive before changing compute packaging.
  The existing renderer remains a separately selected EGL executable.

## Actual build interfaces

The [bundled Viskores wrapper](https://github.com/Kitware/VTK/blob/7c0494a68bff379d32d6b1fbaa3d10d27a73af54/ThirdParty/viskores/vtkviskores/CMakeLists.txt)
maps `VTK_USE_KOKKOS` to `Viskores_ENABLE_KOKKOS`. Its
[device adapter definition](https://github.com/Kitware/VTK/blob/7c0494a68bff379d32d6b1fbaa3d10d27a73af54/ThirdParty/viskores/vtkviskores/viskores/CMake/ViskoresDeviceAdapters.cmake)
requires Kokkos ≥3.7, enables the CMake HIP language when `HIP` is in
`Kokkos_DEVICES`, and creates `viskores_kokkos_hip`.

[VTK accelerator core](https://github.com/Kitware/VTK/blob/7c0494a68bff379d32d6b1fbaa3d10d27a73af54/Accelerators/Vtkm/Core/CMakeLists.txt)
marks device sources as `LANGUAGE HIP` and invokes `kokkos_compilation` when
`viskores::kokkos_hip` exists. The legacy `Vtkm` directory/class names therefore
do not imply absence of the new Viskores implementation.

The same Viskores source's
[HIP CI container](https://github.com/Viskores/viskores/blob/521f3b72aabe0bf37e9972975700df27adbbae71/.gitlab/ci/docker/ubuntu2204_kokkos_hip.dockerfile)
builds Kokkos 3.7.01 and 4.3.01, with HIP and Serial enabled and HIP relocatable
device code disabled. Its configured hardware is GFX908, not the observed
GFX1100. The locked Nixpkgs Kokkos recipe uses 5.2.2 and tests a default CPU
configuration; that package does not establish this older Viskores/HIP ABI.

## Numerical/precision acceptance constraints

- The bundled wrapper sets `Viskores_USE_DOUBLE_PRECISION OFF`. A Float64 field
  does not prove every coordinate or intermediate computation remains Float64.
  An explicit reviewed precision policy and round-trip benchmark are required.
- [vtkmGradient](https://github.com/Kitware/VTK/blob/7c0494a68bff379d32d6b1fbaa3d10d27a73af54/Accelerators/Vtkm/Filters/vtkmGradient.cxx)
  preserves the input topology by shallow copy, supports Float32/Float64 point
  or cell fields, and invokes `viskores::filter::vector_analysis::Gradient`.
  It rejects hidden points/cells and some unstructured cell types. Its cell-field
  path includes explicit point averaging; qualify associations separately.
- The filter falls back to VTK on a Viskores exception unless `ForceVTKm` is
  set. Required compute execution must force the accelerator and independently
  force/observe the HIP/Kokkos device; a class name is insufficient evidence.
- [vtkmThreshold](https://github.com/Kitware/VTK/blob/7c0494a68bff379d32d6b1fbaa3d10d27a73af54/Accelerators/Vtkm/Filters/vtkmThreshold.cxx)
  documents single-precision output points and unsupported magnitude,
  continuous-cell range and hidden-cell operations. It is not an unrestricted
  Float64/topology-preserving substitute.

The initial allowlist candidate is a separately packaged Float64 image-data
point-gradient filter. Qualify linear/quadratic analytical fields, CPU/HIP
agreement, retained OpenLB fields, coordinates/IDs/association, actual kernels,
exact PCI/UUID, sandbox compatibility and RAM/VRAM measurements. Bind its source,
formulation, precision and supported topology in the capability record.

## Retained source evidence

Raw inspected files and their SHA-256 values are recorded in
`/data/scratch/tmp/opencode/harbor-cad-filter-research-20261006/sources.json`.
No dependency pin or qualification status has been changed by this inspection.

## Candidate package wiring

`nix/filters.nix` now defines separate lazy `kokkos-hip` and `vtk-hip` packages.
Kokkos 4.3.01 is pinned at `6ecdf605e0f7639adec599d25cf0e206d7b8f9f5`
with the fetched archive SHA-256
`sha256-X1ApGi78zM48aG3IkPft1FWK+2dJ3kKl9lrq+sWleWc=`. Its upstream
architecture table explicitly contains `AMD_GFX1100`; the package declares it
and disables HIP relocatable device code. Both packages use locked Nixpkgs'
ROCm 7.2.3 compiler/runtime and CMake HIP language support.

The VTK candidate uses the exact existing ParaView archive, verifies the bundled
wrapper/filter source file hashes, and enables accelerator filters plus XML I/O.
It is a separate numerical library build with no EGL, Qt, MPI or Python; the
Float64 wrapper change is scoped to this compute package. Builds do not discover
devices. Guarded evaluation succeeded; the first native build stopped because
Kokkos's default HIP configuration requires rocThrust. Its exact
[TPL finder](https://github.com/kokkos/kokkos/blob/6ecdf605e0f7639adec599d25cf0e206d7b8f9f5/cmake/Modules/FindTPLROCTHRUST.cmake)
also exports rocThrust to consumers, so the corrected package propagates the
locked ROCm dependency. Build logs are retained in
`/data/scratch/tmp/opencode/harbor-cad-filter-research-20261006/`.
These outputs remain `unqualified`; no native filter runtime or B2 result is
claimed by the declaration.

Kokkos builds after propagating rocThrust and its CMake-required rocPRIM
dependency. The next VTK configure exposed a directory-scope mismatch: the
third-party directory's `find_package(Kokkos)` defines the compiler helper,
while sibling accelerator directories cannot see its ordinary compiler variable.
The candidate makes the same pinned ROCm compiler visible through a CMake cache
entry; it retains the compiler existence check and native HIP language.

The exact VTK source's
`Accelerators/Vtkm/DataModel/vtkmlib/ImageDataConverter.cxx` constructs uniform
coordinates from extents/origin/spacing and does not read `GetDirectionMatrix`.
The initial native allowlist therefore rejects non-identity image direction
matrices before HIP selection; the qualifier includes a rotated-image negative
case. No transformed-coordinate numerical qualification is inferred.

The separately packaged `filter-hip` adapter candidate supports one registered
Float64 image-data shard and a point-gradient of `physVelocity` or `physPressure`.
It verifies input hashes and point/allocation bounds, forces both
`vtkmGradient::ForceVTKm` and the Viskores Kokkos device, and explicitly initializes
Kokkos with the PCI/UUID-correlated HIP device. An explicit CPU-reference command
uses VTK for comparison; it cannot satisfy required HIP execution.

Kokkos 4.3.01's
[profiling interface](https://github.com/kokkos/kokkos/blob/6ecdf605e0f7639adec599d25cf0e206d7b8f9f5/core/src/impl/Kokkos_Profiling_Interface.hpp)
defines execution-space/device decoding, and its
[profiling callbacks](https://github.com/kokkos/kokkos/blob/6ecdf605e0f7639adec599d25cf0e206d7b8f9f5/core/src/impl/Kokkos_Profiling.hpp)
provide actual parallel dispatch and allocation events. The candidate rejects
missing dispatches or non-HIP/foreign-device callbacks. It records the observed
dispatch labels, process peak RSS and post-initialization Kokkos HIPSpace peak
allocations. That memory metric excludes driver and non-Kokkos allocations;
whole-card VRAM peaks and instruction-level kernel traces remain separate
qualification work.

Before publishing, the adapter checks its Float64 result, unchanged source array
bytes and image topology/coordinates, then writes and reads back the VTI to check
the same invariants. Original physical units and derivative units are explicit.
Ghost arrays are rejected pending their own qualification. This is candidate
implementation: it needs the guarded native build, analytical/CPU comparison,
sandbox and worker integration gates before being reported as qualified.
The image preallocation check permits bounded signed extents: the retained
OpenLB B1 image uses `-1 16 -1 10 -1 8`, which is a valid image index range.
