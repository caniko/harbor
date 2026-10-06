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
devices. Package evaluation/build is currently blocked by another process owning
the host evaluation guard. These outputs remain `unqualified`; no native filter
runtime or B2 result is claimed by the declaration.
