# Prescribed dry snow insulation reference

`case validate-snow-reference examples/snow-reference.json` and simulation/all
MCP `snow_reference_validate` prepare a bounded series-resistance boundary.
They return `executed: false`. `case plan-snow-reference` and
`case_plan_snow_reference` return the same immutable thermal approval and the
complete boundary preparation. Submit through the existing `job submit` /
`job_submit` path with the returned digest, an authoritative admission policy
and a systemd execution profile.

## Model and original inputs

This synthetic reference prescribes uniform dry snow on **both complete x faces**
of the existing plane-wall thermal fixture. Transverse faces remain insulated.
The fixture explicitly has no openings. Snow thickness, effective conductivity,
density, specific heat, material-temperature domain and provenance are required
SI quantities; bare air-side convection remains explicitly prescribed. No
velocity-to-convection or precipitation-to-coverage inference is made.

For thickness `d`, effective snow conductivity `k_s` and bare coefficient `h`,
the resistance per area is `R = d/k_s + 1/h`, in m² K/W. The existing native
CalculiX `*FILM` plane-wall boundary receives `h_eff = 1/R`, in W/(m² K). This
preserves the same quasi-steady heat flux across the snow and air resistances.
Initial outer snow temperature is
`T_air + (T_device - T_air) * (1/h)/R`. Snow capacity and mass are reported
independently rather than added to the device's stored energy.

The native temperature-history descriptor contains the original snow prescription
and original bare coefficient/provenance under the versioned
`harbor-cad-snow-boundary-v1:` provenance envelope. Rust reconstructs this
envelope during planning, submission and historical scientific verification;
changing the effective coefficient without a matching prescription rejects.
The complete envelope changes the science/approval digest. It uses the supported
version-6 plane-wall solve and its unchanged native runtime/sandbox contract.

## Applicability screens

The approximation omits snow heat storage and requires:

- Snow/device total heat-capacity ratio at most the explicit approved limit,
  itself no larger than 0.02.
- Snow diffusion time `rho_s * cp_s * d² / k_s` divided by the shortest forcing
  knot or retained-observation interval at most the explicit approved limit,
  itself no larger than 0.02.
- A conservative complete-history upper temperature bound, including total
  positive heater energy, strictly below 273.15 K and within the prescribed snow
  material domain. The lower bound uses the minimum initial/ambient temperature.
- Positive finite properties and thickness no larger than the device wall length.

These are applicability screens, not numerical error estimates or physical
validation. The initial snow profile is quasi-steady between the prescribed
device and air temperatures. Partial coverage, missing properties, melting,
deposition, adhesion, variable coverage, radiation, contact resistance and
blocked-opening airflow require separate models and reject here.

The checked example has `R_snow = 0.04`, `R_air = 0.02` m² K/W,
`h_eff = 50/3` W/(m² K), snow/device capacity ratio `2/243`, diffusion time
16 s and diffusion/forcing ratio 0.008. It remains explicitly synthetic.

## Verification and qualification

The native thermal pipeline preserves original Float64 DAT, meshes, point
temperatures, physical times and portable exports. Historical numerical evidence
retains the prescribed snow identity and the omitted-storage screens alongside
the independent plane-wall temperature and cumulative-energy gates (both 0.02).
Temperature sampling, comparison, surface moisture inspection and conservative
thermal projection continue through their normal source-bound result operations.

The dedicated native campaign is:

```sh
python3 scripts/verify_snow_cpu.py \
  --executable CLI --runtime THERMAL_REFERENCE_RUNTIME --output NEW_DIRECTORY
```

Run under the normal Atlas runtime lease and a bounded service (2 GiB/no swap,
two CPUs, 128 tasks). It independently reconstructs the native DAT temperature
and energy gates, verifies closure-only sandbox canaries, requires decreasing
equal-time spatial errors and separate fixed-mesh temporal convergence, compares
snow with the bare reference, and retains unsupported-input rejections. An
explicit `--development-planner` flag labels source-built planner diagnostics
as package-unqualified. Exact packaged native and worker qualification remain
pending normal build headroom; full CPU tests do not establish them.

`--worker-runtime THERMAL_WORKER_RUNTIME` explicitly selects an already realized
operation-only CPU thermal worker descriptor instead of `--runtime`. It must
retain the exact thermal adapter/closure and have no other active operations.
The descriptor kind and closure checksum are recorded. This uses the same native
thermal executable, independent original-field checks and closure-only sandbox;
it does not qualify worker execution or a development planner. An older
standalone descriptor lacking its closure still refuses before output creation.
