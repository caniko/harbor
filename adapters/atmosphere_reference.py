"""Pinned libRadtran DISORT UV flux and full ordered angular radiance bridge.

Semantics come from libRadtran 2.0.6 src_py/{spectral,geometry,output}_options.py.
The distribution is content-pinned; original native text remains authoritative.
"""

import argparse
import hashlib
import json
import math
import os
import subprocess
from itertools import pairwise
from pathlib import Path

SOURCE_SHA256 = "64930cc40b6e4a37aa220520974d330fc1563796f466a649b2238131f2d69840"
POLICY = "harbor-cad-atmosphere-cpu-v1"


def strict_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate atmospheric request field")
        result[key] = value
    return result


def number(value):
    if type(value) not in (int, float) or not math.isfinite(value):
        raise ValueError("explicit finite atmospheric scalar required")
    return float(value)


def quantity(value, units):
    if (
        not isinstance(value, dict)
        or set(value) != {"value", "unit"}
        or value["unit"] not in units
    ):
        raise ValueError("explicit supported atmospheric spectral units required")
    return number(value["value"]) * units[value["unit"]]


def normalize(spec):
    keys = {
        "schema_version",
        "synthetic",
        "backend",
        "solver",
        "model",
        "profile",
        "wavelengths",
        "toa_irradiance",
        "solar_zenith_deg",
        "solar_azimuth_deg",
        "albedo",
        "streams",
        "mu_bins",
        "phi_bins",
        "relative_tolerance",
        "source_provenance",
        "atmosphere_provenance",
    }
    if (
        not isinstance(spec, dict)
        or set(spec) != keys
        or type(spec["schema_version"]) is not int
        or spec["schema_version"] != 1
        or spec["synthetic"] is not True
        or spec["backend"] != "cpu"
        or spec["solver"] != "disort"
        or spec["model"] not in ("clear_sky_molecular_crs", "transparent_reference")
        or spec["profile"] not in ("afglms", "afglmw")
    ):
        raise ValueError(
            "strict explicit synthetic pinned DISORT molecular UV or transparent reference required"
        )
    for name in ("streams", "mu_bins", "phi_bins"):
        if type(spec[name]) is not int or spec[name] not in (8, 16, 32, 64):
            raise ValueError(
                "bounded explicit native streams and full angular midpoint grid required"
            )
    zenith = number(spec["solar_zenith_deg"])
    azimuth = number(spec["solar_azimuth_deg"])
    albedo = number(spec["albedo"])
    tolerance = number(spec["relative_tolerance"])
    if (
        not 0 <= zenith <= 80
        or not 0 <= azimuth < 360
        or not 0 <= albedo <= 1
        or spec["model"] == "transparent_reference"
        and albedo != 0
        or not 0 < tolerance <= 0.02
    ):
        raise ValueError(
            "resolved solar direction, explicit albedo and unchanged analytic/angular flux gate required"
        )
    if any(
        not isinstance(spec[k], str)
        or not spec[k].strip()
        or len(spec[k].encode()) > 2048
        for k in ("source_provenance", "atmosphere_provenance")
    ):
        raise ValueError("explicit solar and atmospheric provenance required")
    if (
        not isinstance(spec["wavelengths"], list)
        or not isinstance(spec["toa_irradiance"], list)
        or not 2 <= len(spec["wavelengths"]) <= 64
        or len(spec["toa_irradiance"]) != len(spec["wavelengths"])
    ):
        raise ValueError("complete bounded ordered UV original source knots required")
    wavelengths = [
        quantity(q, {"nm": 1.0, "m": 1e9, "mm": 1e6}) for q in spec["wavelengths"]
    ]
    toa = [
        quantity(q, {"W/(m2*nm)": 1.0, "W/(m2*m)": 1e-9})
        for q in spec["toa_irradiance"]
    ]
    if (
        any(not 280 - 1e-12 <= w <= 400 + 1e-12 for w in wavelengths)
        or any(abs(w * 1000 - round(w * 1000)) > 1e-6 for w in wavelengths)
        or any(round(b * 1000) <= round(a * 1000) for a, b in pairwise(wavelengths))
        or any(not 0 <= v <= 1e6 for v in toa)
        or not any(toa)
    ):
        raise ValueError(
            "supported molecular-cross-section band 280–400 nm on native 0.001 nm decimal-output grid and finite nonnegative nonzero UV source required"
        )
    zenith = math.radians(zenith)
    azimuth = math.radians(azimuth)
    return {
        "schema_version": 1,
        "executed": False,
        "source_sha256": SOURCE_SHA256,
        "wavelengths_nm": wavelengths,
        "toa_irradiance_w_m2_nm": toa,
        "propagation_direction": [
            math.sin(zenith) * math.sin(azimuth),
            math.sin(zenith) * math.cos(azimuth),
            -math.cos(zenith),
        ],
        "umu": [-1 + (i + 0.5) / spec["mu_bins"] for i in range(2 * spec["mu_bins"])],
        "phi_deg": [
            (i + 0.5) * 360 / spec["phi_bins"] for i in range(spec["phi_bins"])
        ],
        "angular_cell_solid_angle_sr": 2
        * math.pi
        / (spec["mu_bins"] * spec["phi_bins"]),
        "transparent_horizontal_reference_w_m2_nm": [v * math.cos(zenith) for v in toa]
        if spec["model"] == "transparent_reference"
        else None,
        "physical_validation": "unqualified",
    }


def parse_original(text, spec, prepared):
    columns = 4 + len(prepared["umu"]) * len(prepared["phi_deg"])
    rows = text.splitlines()
    if len(rows) != len(prepared["wavelengths_nm"]):
        raise ValueError("exact native wavelength/altitude row coverage required")
    direct = []
    down = []
    up = []
    radiance = []
    maximum = 0.0
    for row, wl, toa in zip(
        rows,
        prepared["wavelengths_nm"],
        prepared["toa_irradiance_w_m2_nm"],
        strict=True,
    ):
        values = [float(v) for v in row.split()]
        if (
            len(values) != columns
            or any(not math.isfinite(v) or v < 0 for v in values)
            or values[0] != round(wl * 1000) / 1000
        ):
            raise ValueError(
                "complete finite original wavelength, direct/diffuse flux and ordered full-sphere radiance columns required"
            )
        original = values[4:]
        radiance.append(original)
        direct.append(values[1])
        down.append(values[2])
        up.append(values[3])
        expected_direct = toa * (-prepared["propagation_direction"][2])
        # Direct light cannot exceed the prescribed incident source in this
        # absorption/scattering model. Keep zero-source rows exactly zero.
        if (
            values[1] > expected_direct * (1 + spec["relative_tolerance"])
            or toa == 0
            and any(values[1:])
        ):
            raise ValueError(
                "native direct irradiation exceeds its prescribed source or creates energy from a zero source"
            )
        expected_up = spec["albedo"] * (values[1] + values[2])
        if expected_up == 0:
            if values[3] != 0:
                raise ValueError(
                    "black sea-level boundary must have zero native upward diffuse flux"
                )
        elif abs(values[3] / expected_up - 1) > spec["relative_tolerance"]:
            raise ValueError(
                "native Lambertian boundary reflected-flux conservation failed"
            )
        for sign, flux in ((-1, values[2]), (1, values[3])):
            reconstructed = math.fsum(
                value * abs(mu) * prepared["angular_cell_solid_angle_sr"]
                for i, mu in enumerate(prepared["umu"])
                if mu * sign > 0
                for value in original[i * spec["phi_bins"] : (i + 1) * spec["phi_bins"]]
            )
            if flux == 0:
                if reconstructed != 0:
                    raise ValueError(
                        "zero native diffuse flux must retain zero angular radiance"
                    )
                error = 0.0
            else:
                error = abs(reconstructed / flux - 1)
            if error > spec["relative_tolerance"]:
                raise ValueError(
                    "original angular diffuse integration unresolved at unchanged native-flux gate"
                )
            maximum = max(maximum, error)
        if spec["model"] == "transparent_reference":
            expected = toa * (-prepared["propagation_direction"][2])
            if expected == 0:
                if values[1] != 0:
                    raise ValueError(
                        "zero source must remain zero in original native fields"
                    )
                error = 0.0
            else:
                error = abs(values[1] / expected - 1)
            if error > spec["relative_tolerance"] or any(v != 0 for v in values[2:]):
                raise ValueError(
                    "transparent analytic irradiance/zero diffuse gate failed"
                )
            maximum = max(maximum, error)
    return {
        "direct_horizontal_w_m2_nm": direct,
        "direct_normal_w_m2_nm": [
            v / (-prepared["propagation_direction"][2]) for v in direct
        ],
        "diffuse_downward_w_m2_nm": down,
        "diffuse_upward_w_m2_nm": up,
        "radiance_w_m2_sr_nm": radiance,
        "angular_order": "umu-major phi-minor; full original propagation sphere; negative umu downwelling",
        "maximum_angular_flux_error": maximum,
        "tolerance": spec["relative_tolerance"],
        "physical_validation": "unqualified",
    }


def sandbox_checks(request):
    names = {
        "operation_closure_only": Path("/atmosphere-runtime-closure.txt").is_file(),
        "no_gpu_nodes": not Path("/dev/dri").exists() and not Path("/dev/kfd").exists(),
        "no_sysfs": not Path("/sys/class/drm").exists(),
        "no_host_home": os.environ.get("HOME") == "/home/worker"
        and list(Path("/home").iterdir()) == [Path("/home/worker")],
        "no_session_bus": "DBUS_SESSION_BUS_ADDRESS" not in os.environ,
        "no_worker_socket": "HARBOR_CAD_SOCKET" not in os.environ,
        "network_namespace_isolated": bool(os.environ.get("HARBOR_CAD_HOST_NETNS"))
        and os.readlink("/proc/self/ns/net") != os.environ.get("HARBOR_CAD_HOST_NETNS"),
    }
    readonly = False
    try:
        descriptor = os.open(request, os.O_WRONLY)
        os.close(descriptor)
    except OSError as error:
        readonly = error.errno in (13, 30)
    names["descriptor_readonly"] = readonly
    if os.environ.get("HARBOR_CAD_ATMOSPHERE_POLICY") != POLICY or not all(
        names.values()
    ):
        raise ValueError(
            "complete operation-specific immutable CPU atmospheric isolation required"
        )
    return {"policy": POLICY, "checks": names}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=["reference"])
    parser.add_argument("request", type=Path)
    args = parser.parse_args()
    if (
        args.request.is_symlink()
        or not args.request.is_file()
        or args.request.stat().st_size > 65536
    ):
        raise ValueError("bounded regular atmospheric request required")
    raw = args.request.read_bytes()
    spec = json.loads(raw, object_pairs_hook=strict_object)
    prepared = normalize(spec)
    root = Path.cwd()
    if any(
        p.name != "atmosphere.log" or p.is_symlink() or not p.is_file()
        for p in root.iterdir()
    ):
        raise ValueError(
            "new native atmospheric work directory with only an owned stage log permitted"
        )
    isolation = sandbox_checks(args.request)
    binary = Path(os.environ["HARBOR_CAD_LIBRADTRAN"]).resolve(strict=True)
    data = Path(os.environ["HARBOR_CAD_LIBRADTRAN_DATA"]).resolve(strict=True)
    if (
        not binary.is_relative_to("/nix/store")
        or not binary.is_file()
        or not data.is_relative_to("/nix/store")
        or not data.is_dir()
    ):
        raise ValueError(
            "immutable exact native libRadtran binary/data package required"
        )
    solar = root / "solar-spectrum.dat"
    grid = root / "wavelengths.dat"
    deck = root / "uvspec-input.inp"
    solar.write_text(
        "".join(
            f"{wl:.17e} {toa:.17e}\n"
            for wl, toa in zip(
                prepared["wavelengths_nm"],
                prepared["toa_irradiance_w_m2_nm"],
                strict=True,
            )
        )
    )
    grid.write_text("".join(f"{wl:.17e}\n" for wl in prepared["wavelengths_nm"]))
    lines = [
        f"data_files_path {data}",
        f"atmosphere_file {data}/atmmod/{spec['profile']}.dat",
        f"source solar {solar} per_nm",
        f"wavelength {prepared['wavelengths_nm'][0]:.17e} {prepared['wavelengths_nm'][-1]:.17e}",
        f"wavelength_grid_file {grid}",
        "mol_abs_param crs",
        f"sza {spec['solar_zenith_deg']:.17e}",
        f"phi0 {spec['solar_azimuth_deg']:.17e}",
        f"albedo {spec['albedo']:.17e}",
        "rte_solver disort",
        f"number_of_streams {spec['streams']}",
        "zout 0",
        "umu " + " ".join(f"{v:.17e}" for v in prepared["umu"]),
        "phi " + " ".join(f"{v:.17e}" for v in prepared["phi_deg"]),
        "output_user lambda edir edn eup uu",
        "quiet",
    ]
    if spec["model"] == "transparent_reference":
        lines.extend(["no_absorption", "no_scattering"])
    deck.write_text("\n".join(lines) + "\n")
    output = root / "uvspec-original.txt"
    with (
        deck.open("rb") as inputs,
        output.open("xb") as stdout,
        (root / "uvspec-native.log").open("xb") as stderr,
    ):
        result = subprocess.run(
            [str(binary)],
            stdin=inputs,
            stdout=stdout,
            stderr=stderr,
            check=False,
            timeout=300,
            env={
                "PATH": "/nonexistent",
                "HOME": "/home/worker",
                "LC_ALL": "C",
                "OMP_NUM_THREADS": "2",
            },
        )
    if result.returncode:
        raise RuntimeError(
            f"native libRadtran failed with {result.returncode}; original partial fields/logs retained"
        )
    if output.stat().st_size > 64 * 1024 * 1024:
        raise ValueError("bounded complete native atmospheric original output required")
    parsed = parse_original(output.read_text(), spec, prepared)
    receipt = {
        "schema_version": 1,
        "adapter": "libRadtran",
        "source_sha256": SOURCE_SHA256,
        "version": "2.0.6",
        "backend": "cpu",
        "solver": "disort",
        "precision": "Float32",
        "reduction_precision": "Float64",
        "original_serialization": "unchanged native Float32 spectral fields; wavelength %.3f, flux %.6e, radiance %.9e; no precision promotion",
        "executed": True,
        "software_fallback": False,
        "input": spec,
        "request_sha256": hashlib.sha256(raw).hexdigest(),
        "sandbox": isolation,
        "prepared": prepared,
        "observations": parsed,
        "original": {
            "path": output.name,
            "bytes": output.stat().st_size,
            "sha256": hashlib.sha256(output.read_bytes()).hexdigest(),
        },
        "physical_validation": "unqualified",
        "limitations": [
            "synthetic AFGL sea-level molecular-only UV reference",
            "no site/weather/aerosol/cloud qualification",
            "complete angular original radiances retained; no diffuse-isotropic approximation",
            "DISORT stream, wavelength and angular refinement assessed separately",
            "no Mitsuba/GPU/physical transport qualification",
        ],
    }
    with (root / "atmosphere-receipt.json").open("x") as handle:
        json.dump(receipt, handle, indent=2, allow_nan=False)


if __name__ == "__main__":
    main()
