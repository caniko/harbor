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

Angular integration consistency is numerical verification. It does not establish
stream, angular-shape or wavelength convergence. Complete original radiances
remain anisotropic observations; conversion to an isotropic sky is unsupported.

## Operation isolation and qualification

`nix/atmosphere.nix` provides `atmosphere-native-cpu`, `atmosphere-reference-cpu`
and `runtime-atmosphere-reference-cpu`. A native transparent package smoke test
is distinct from the complete atmospheric campaign. The adapter requires its
own `harbor-cad-atmosphere-cpu-v1` Bubblewrap closure, read-only descriptor and
eight measured CPU isolation checks, including changed network namespace and
absent GPU/home/session/worker access. It generates a closed whitelisted input
deck, retains original native text/logs and fails on unresolved or altered fields.

The CLI command is `case validate-atmospheric-reference FILE`; the simulation MCP
tool is `atmospheric_reference_validate`. These validate without executing or
creating a job. Native atmospheric worker integration and direct/diffuse angular
handoff into Mitsuba remain separate acceptance work. Site-specific weather,
solar history, aerosol/cloud profiles, measured optics, physical ageing/damage
and GPU transport remain unqualified.
