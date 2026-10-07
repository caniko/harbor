# Explicit molecular UV atmospheric reference

This slice prepares a synthetic sea-level atmospheric UV reference through the
same Rust CLI/MCP validation contract and packages the public CPU DISORT solver.
Native execution, convergence and physical validation are separate evidence
states. Exact packaged native qualification is pending.

## Immutable upstream contract

The official [libRadtran 2.0.6 distribution](https://www.libradtran.org/download/libRadtran-2.0.6.tar.gz)
has 154,147,176 bytes and SHA-256
`64930cc40b6e4a37aa220520974d330fc1563796f466a649b2238131f2d69840`.
It is GPL-2.0-or-later; the public `uvspec` target and CPU C DISORT implementation
are built with the locked Nixpkgs C/Fortran/NetCDF toolchain. The isolated adapter
uses Python 3.13 without a solver Python extension. Source/data identities and
the inspected lines are recorded in
[the source evidence](evidence/atmosphere-source-20261007.json).

The distribution's `README` still labels itself 0.99-alpha. Preparation uses the
content-pinned 2.0.6 archive, actual parser, current option documentation and
build files rather than that stale introduction.

- `src_py/spectral_options.py:206–291`: a caller-provided two-column extraterrestrial
  spectrum, explicit `per_nm` and prescribed wavelength grid preserve source
  units. `src/uvspec_lex.c:13906–13913` documents 1 AU with no day-of-year correction
  when omitted; the reference uses this explicit convention.
- `src_py/geometry_options.py:886–948`: `phi0=0` means sun in the South,
  `90` means West. The retained propagation vector in East/North/Up coordinates
  is `(sin(sza)sin(phi0), sin(sza)cos(phi0), -cos(sza))`.
  Native `umu` is negative for downwelling radiance and positive for upwelling;
  zero is forbidden. Output `phi` describes sensor position around the vertical:
  North `0`, East `90`, South `180`, West `270`.
- `src_py/output_options.py:637–658` and `src/uvspec_lex.c:20124–20208`:
  `output_user lambda edir edn eup uu` retains direct horizontal irradiance,
  downward/upward diffuse irradiance and `umu`-major, `phi`-minor radiances.
  Wavelength text is `%.3f`, flux `%.6e`, radiance `%.9e`.
- `src/uvspec.h:1398,1935–1945`: retained spectral wavelength, flux and radiance
  arrays are native Float32. Python Float64 parsing/reduction does not upgrade
  that native precision. Original text is retained without patching serialization.
- `src_py/molecular_options.py:667–669`: `mol_abs_param crs` disables spectral
  parameterizations and uses molecular cross sections. The reference accepts
  280–400 nm on the native 0.001 nm decimal grid. It requires 2–64 strictly ordered
  knots, complete explicit nonnegative source values and provenance.

## Inputs and numerical gates

`AtmosphericReferenceSpec` is strict. `clear_sky_molecular_crs` selects the pinned
AFGL midlatitude summer (`afglms`) or winter (`afglmw`) profile, default molecular
cross sections, explicit Lambertian ground albedo, no aerosols/clouds, sea level
and a caller-prescribed top-of-atmosphere spectrum. Solar zenith is 0–80 degrees;
streams and midpoint angular bins are explicit bounded values 8/16/32/64.
Preparation returns `executed=false` and no invented analytical atmospheric
solution. `transparent_reference` explicitly switches off absorption/scattering,
requires a black boundary and supplies the separate cosine-law direct reference.

The complete angular grid covers the original propagation sphere. Each midpoint
cell spans `dOmega=2*pi/(mu_bins*phi_bins)`. For each wavelength and hemisphere,
independent `fsum(L_lambda*abs(umu)*dOmega)` must reproduce original native diffuse
flux within the explicit gate, at most 0.02. Direct irradiation must not exceed
its prescribed input; zero-source rows remain zero. Original upward diffuse flux
must equal `albedo*(edir+edn)`. Transparent direct flux must match the analytic
cosine reference and original diffuse flux/radiance must be zero.
Net surface power `edir+edn-eup` must not exceed prescribed TOA horizontal
irradiance. This separately rejects internally consistent diffuse fields that
create energy, while permitting the downward enhancement from multiple reflection.

Angular integration consistency is numerical verification. It does not establish
stream, angular-shape or wavelength convergence. Complete original radiances
remain anisotropic observations; conversion to an isotropic sky is unsupported.

## Operation isolation and qualification

`nix/atmosphere.nix` provides `atmosphere-native-cpu`, `atmosphere-reference-cpu`,
`runtime-atmosphere-reference-cpu` and `runtime-atmosphere-worker`. A native transparent package smoke test
is distinct from the complete atmospheric campaign. The adapter requires its
own `harbor-cad-atmosphere-cpu-v1` Bubblewrap closure, read-only descriptor and
eight measured CPU isolation checks, including changed network namespace and
absent GPU/home/session/worker access. It generates a closed whitelisted input
deck, retains original native text/logs and fails on unresolved or altered fields.
The selected AFGL profile must match its original official SHA-256 before launch;
the receipt also retains the exact packaged native executable path/hash. Full
angular arrays stay in the authoritative native text artifact, with bounded
flux summaries and original field shape/order/unit metadata in the receipt.

The CLI command `case validate-atmospheric-reference FILE` and simulation MCP
tool `atmospheric_reference_validate` validate without executing or creating a
job. `case plan-atmospheric-reference FILE` and `atmospheric_reference_plan`
return the same independent version-14 `atmosphere → bundle` approval. Native
submission requires the shared same-user authority and tracked systemd service.
The complete angular sphere and bounded decimal/native parser copies are reserved
before admission. Older approvals reject injected atmospheric fields, including
null; atmosphere-only sandbox policy cannot reuse spectral or generic CPU policy.

`src/atmosphere_fields.rs` independently verifies original text row coverage,
units/shape, cosine-law/direct/ground/source energy, both hemisphere integrals,
explicit gates, source/profile identities and complete isolation evidence before
ingestion. Originals have registered units and wavelength/solid-angle association,
with no invented physical time. Historical numerical verification rechecks the
registered original bytes; manufactured numerical fixtures do not establish
runtime execution. The worker additionally rechecks the actual immutable native
executable hash at execution. Runtime, convergence and physical validation retain
separate states.

Exact packaged atmospheric worker execution and direct/diffuse angular
handoff into Mitsuba remain separate acceptance work. Site-specific weather,
solar history, aerosol/cloud profiles, measured optics, physical ageing/damage
and GPU transport remain unqualified.

`scripts/verify_atmosphere_cpu.py` requires exact packaged planner/runtime paths.
It independently reconstructs original hemisphere flux, checks three transparent
solar geometries and explicit 0.6-albedo conservation, and assesses three separate
refinements: DISORT streams 16/32/64, angular midpoint grids 16/32/64 and nested
4/2/1 nm spectral sampling over a prescribed 300–360 nm sub-band. Coarse angular
shape is compared against corresponding fine native cells with solid-angle
averaging; no isotropic replacement is used. Relative-L2 errors against the finest
retain the same at-most-0.02 gate and must decrease, allowing an explicitly
recorded at-most-1e-7 native-serialization plateau. Eight invalid inputs must reject
before fields appear. The original native data, commands, exit codes, failed
attempts, reconstructed receipts, resources and separate refinement assessments
are retained. The campaign is implemented and remains unexecuted.

`scripts/verify_atmosphere_worker.py` requires a complete matching exact-package
standalone report and unchanged originals, including all three independent
refinements. It runs CLI and real MCP approval/submission, full angular original
reconstruction/parity, registered mutation refusal, offline checksum export,
tracked resource limits, worker restart, native-tree forced death and cancellation,
and reservation/runtime-root release. Four plan/envelope/approval rejections are
checked. The shared lifecycle API/signature regression covers this entrypoint.
This worker campaign remains unexecuted until exact realization and the guarded
Atlas execution lease are available.

## Original anisotropic source handoff

`results transfer-atmosphere REQUEST.json` and the results-profile MCP tool
`results_transfer_atmosphere` prepare the same source-bound one-way descriptor.
`examples/atmosphere-transfer.json` needs an actual qualified source job ID. The
worker requires its succeeded native atmospheric exit, immutable execution
binding, reported numerical verification and unchanged registered originals.
Manufactured or unexecuted sources cannot supply a transfer.

The receiver's directional source binds the original prescribed **TOA** spectrum
and solar direction. The proposed ground illumination comes solely from original
`edir` and `uu`: direct normal irradiance is `edir/cos(sza)`; each diffuse original
midpoint contributes `uu*dOmega` normal irradiance along
`(sqrt(1-umu²)*sin(phi), sqrt(1-umu²)*cos(phi), umu)` in East/North/Up coordinates.
This convention follows the original native sensor-position azimuth semantics.
Both hemispheres and every original wavelength are retained, without isotropic,
RGB, clipping, flux-rescaling or angular-interpolation substitutions. Every
proposed emitter amount is back-reconstructed into original units under the
explicit transfer gate (at most `1e-10`); native angular integration and future
transport/sampling accuracy have separate gates.

The bounded report includes original field/receipt identities and an unobstructed
planar midpoint-quadrature reference at the explicit receiver normal. Exact
original-knot integration separately computes incident, absorption-weighted and
ageing-weighted irradiance, area-dependent power and prescribed-history dose.
Radiance arrays stay in the authoritative artifact. The report is a transfer
preparation (`executed=false`, `native_transport=not_executed`), with native
Mitsuba transport and physical qualification remaining separate work.

## Native original-midpoint transport reference

`adapters/atmospheric_spectral.py` maps every original angular cell into a
Mitsuba 3.9.1 directional emitter, using the isolated Dr.Jit 1.5.0/Python 3.13
ABI. Direct and diffuse illumination use separate native scenes. Original
zero cells remain in the source manifest; only completely zero emitters are
omitted from sampling. No angular interpolation or isotropic replacement occurs.

The pinned upstream [`directional.cpp`](https://github.com/mitsuba-renderer/mitsuba3/blob/478e193a183c21723f4a8251afc3ad29a8da4c5e/src/emitters/directional.cpp)
defines irradiance normal to propagation and returns the opposite, source-facing
sample direction. [`scene.cpp`](https://github.com/mitsuba-renderer/mitsuba3/blob/478e193a183c21723f4a8251afc3ad29a8da4c5e/src/render/scene.cpp)
selects emitters with source-only importance weights, multiplies their direction
PDF by the emitter PMF and divides the returned spectral weight by that PMF.
Its empty-emitter branch returns a zero direction record and spectrum.
[`distr_1d.h`](https://github.com/mitsuba-renderer/mitsuba3/blob/478e193a183c21723f4a8251afc3ad29a8da4c5e/include/mitsuba/core/distr_1d.h)
accumulates scalar-distribution weights in double precision before storing the
Float32 normalization. These APIs permit multiple directional sources without
turning them into an environment map.

Native observations retain four-wavelength packets with source-facing direction,
surface position, cosine, emitter identity, PDF and original Float32 weights.
Every original knot is evaluated, including repeated padding in the final packet.
Compensated Float64 reduction and exact original-knot optical-product quadrature
keep incident, absorption-weighted and ageing-weighted irradiance separate from
area-dependent power and prescribed-history exposure/energy. Native shape area
must preserve the explicit SI dimensions within the `5e-6` Float32 allowance;
the original-to-emitter conservation limit remains `1e-10`.

`nix/spectral.nix` exports `atmospheric-spectral-reference-cpu` and
`runtime-atmospheric-spectral-reference-cpu`. The executable requires
`harbor-cad-atmospheric-spectral-cpu-v1`, its operation-only immutable closure,
a read-only request and checksum-bound read-only atmospheric originals at
`/inputs/atmosphere-original.txt`. Nine measured sandbox canaries are required.
Its receipt does not authorize atmospheric source execution or a worker job.

`scripts/verify_atmospheric_spectral_cpu.py --runtime RUNTIME --output NEW`
implements nine manufactured-source native cases: diffuse opposing normals,
azimuth sectors, reflected upwelling, positive direct incidence, inclined
receivers, mixed direct/diffuse illumination, empty diffuse scenes and a varying
six-knot spectrum with separate optical curves and zero knots. Each seed must
have exactly one canonical direct and diffuse original packet artifact.
The verifier independently checks both receiver-local rectangle coordinates,
source-derived PMFs, packet coverage, original directions and spectral weights,
then combines reconstructed component values. An empty originals list, duplicated
component, renamed artifact or compensated PDF/weight substitution rejects.

`--development-site SITE` explicitly selects an extracted-wheel native API
diagnostic. Neither manufactured source fields nor such a diagnostic qualify
libRadtran execution, exact production packaging, registered-source worker
transport, convergence, GPU transport or physical validation. Exact packaged
execution and a source-bound durable transport approval remain separate gates.
