"""Native atmospheric-midpoint transport diagnostics, without atmosphere promotion.

Inputs are manufactured analytical fields, not native libRadtran execution.
Exact renderer packaging, native transport, source provenance and physical
validation remain separately recorded. Registered-source worker work is pending.
"""

import argparse
import copy
import csv
import json
import math
import os
import subprocess
import sys
from pathlib import Path

from verify_openlb_hip import service_resources
from verify_spectral_cpu import checksum


def original_fields(atmosphere, direct_normal=0.0, sector=False, *, diffuse_scale=1.0):
    n = len(atmosphere["wavelengths"])
    direct_values = (
        direct_normal if isinstance(direct_normal, list) else [direct_normal] * n
    )
    diffuse_values = (
        diffuse_scale if isinstance(diffuse_scale, list) else [diffuse_scale] * n
    )
    rows = []
    for wavelength, direct, diffuse in zip(
        atmosphere["wavelengths"], direct_values, diffuse_values, strict=True
    ):
        horizontal = direct * math.cos(math.radians(atmosphere["solar_zenith_deg"]))
        down = diffuse * (math.pi / atmosphere["phi_bins"] if sector else math.pi)
        up = atmosphere["albedo"] * (horizontal + down)
        angular = [
            diffuse if not sector or j == 0 else 0.0
            for _ in range(atmosphere["mu_bins"])
            for j in range(atmosphere["phi_bins"])
        ]
        angular.extend(
            [up / math.pi] * (atmosphere["mu_bins"] * atmosphere["phi_bins"])
        )
        rows.append(
            " ".join(
                map(str, [f"{wavelength['value']:.3f}", horizontal, down, up, *angular])
            )
        )
    return "\n".join(rows) + "\n"


def reference_cases(repo):
    base = json.loads((repo / "examples/atmosphere-reference.json").read_text())
    base.update(mu_bins=8, phi_bins=8)
    template = json.loads((repo / "examples/atmosphere-transfer.json").read_text())[
        "receiver"
    ]
    for values in (base["toa_irradiance"], template["source"]["irradiance"]):
        for value in values:
            value["value"] = 1000.0
    template["samples"] = 65536
    cases = []
    for name, normal, direct, diffuse, sector, albedo, rich in (
        ("diffuse-up", [0, 0, 1], 0.0, 1.0, False, 0.0, False),
        ("diffuse-down", [0, 0, -1], 0.0, 1.0, False, 0.0, False),
        ("sector-south", [0, -1, 0], 0.0, 1.0, True, 0.0, False),
        ("sector-north", [0, 1, 0], 0.0, 1.0, True, 0.0, False),
        ("mixed-reflected-down", [0, 0, -1], 10.0, 1.0, False, 0.5, False),
        ("direct-up", [0, 0, 1], 10.0, 0.0, False, 0.0, False),
        ("direct-inclined", [0.6, 0, 0.8], 10.0, 0.0, False, 0.0, False),
        ("mixed-up", [0, 0, 1], 10.0, 1.0, False, 0.5, False),
        (
            "multiknot-inclined",
            [0.6, 0, 0.8],
            [10, 0, 30, 20, 50, 5],
            [1, 2, 0, 3, 4, 1],
            False,
            0.4,
            True,
        ),
    ):
        atmosphere = {**copy.deepcopy(base), "albedo": albedo}
        receiver = {**copy.deepcopy(template), "sensor_normal": normal}
        if rich:
            for spec in (atmosphere, receiver):
                spec["wavelengths"] = [
                    {"value": 280 + 24 * k, "unit": "nm"} for k in range(6)
                ]
            atmosphere["toa_irradiance"] = [
                {"value": 1000.0, "unit": "W/(m2*nm)"} for _ in range(6)
            ]
            receiver["source"]["irradiance"] = copy.deepcopy(
                atmosphere["toa_irradiance"]
            )
            receiver.update(
                absorptivity=[0.1, 0.8, 0.3, 0.6, 0.4, 0.9],
                ageing_action=[0.9, 0.2, 0.7, 0.1, 0.8, 0.3],
            )
        cases.append(
            (
                name,
                atmosphere,
                receiver,
                original_fields(atmosphere, direct, sector, diffuse_scale=diffuse),
            )
        )
    return cases


def reconstruct(path, receiver, normalized, component, verifier):
    if component not in ("direct", "diffuse"):
        raise ValueError("exact direct or diffuse native component required")
    wavelengths = normalized["wavelengths_nm"]
    emitters = (
        [normalized["direct_emitter"]]
        if component == "direct"
        else normalized["diffuse_emitters"]
    )
    by_id = {e["id"]: e for e in emitters if any(e["irradiance_w_m2_nm"])}
    maximum = max((max(e["irradiance_w_m2_nm"]) for e in by_id.values()), default=0.0)
    sampling_weights = {
        identity: math.fsum(v / maximum for v in e["irradiance_w_m2_nm"])
        / len(wavelengths)
        for identity, e in by_id.items()
    }
    total_weight = math.fsum(sampling_weights.values())
    expected_pmf = {
        identity: weight / total_weight for identity, weight in sampling_weights.items()
    }
    columns = [
        "sample",
        "knot_offset",
        "x_m",
        "y_m",
        "z_m",
        "towards_source_x",
        "towards_source_y",
        "towards_source_z",
        "native_cosine",
        "native_emitter_id",
        "native_pdf",
        *[f"native_weight_w_m2_nm_{i}" for i in range(4)],
    ]
    sums = [[] for _ in wavelengths]
    normal = receiver["sensor_normal"]
    width, height = normalized["sensor_size_m"]
    up = [0, 1, 0] if abs(normal[1]) < 0.9 else [1, 0, 0]

    def cross(a, b):
        return [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]

    def dot(a, b):
        return math.fsum(x * y for x, y in zip(a, b, strict=True))

    right = cross(up, normal)
    right = [v / math.sqrt(dot(right, right)) for v in right]
    vertical = cross(normal, right)
    geometric_roundoff = 5e-6 * max(width, height)
    with path.open() as handle:
        rows = csv.DictReader(handle)
        if rows.fieldnames != columns:
            raise ValueError("exact native packet columns required")
        for sample in range(receiver["samples"]):
            for offset in range(0, len(wavelengths), 4):
                row = next(rows, None)
                if (
                    row is None
                    or int(row["sample"]) != sample
                    or int(row["knot_offset"]) != offset
                ):
                    raise ValueError(
                        "complete ordered original native sample/knot packets required"
                    )
                point = [float(row[f"{axis}_m"]) for axis in "xyz"]
                direction = [float(row[f"towards_source_{axis}"]) for axis in "xyz"]
                cosine = float(row["native_cosine"])
                pdf = float(row["native_pdf"])
                weights = [float(row[f"native_weight_w_m2_nm_{i}"]) for i in range(4)]
                if (
                    any(
                        not math.isfinite(v)
                        for v in [*point, *direction, cosine, pdf, *weights]
                    )
                    or any(v < 0.0 for v in [cosine, pdf, *weights])
                    or cosine > 1.000005
                    or abs(dot(point, normal)) > geometric_roundoff
                    or abs(dot(point, right)) > width / 2 + geometric_roundoff
                    or abs(dot(point, vertical)) > height / 2 + geometric_roundoff
                ):
                    raise ValueError(
                        "original finite native Float32 surface geometry, weights and cosine required"
                    )
                emitter = by_id.get(row["native_emitter_id"])
                if emitter is None:
                    if (
                        by_id
                        or row["native_emitter_id"] != "none"
                        or any(weights)
                        or cosine != 0.0
                        or pdf != 0.0
                    ):
                        raise ValueError(
                            "only an empty exact source may yield the empty native emitter"
                        )
                else:
                    expected = emitter["propagation_direction"]
                    if (
                        any(
                            abs(a + b) > 5e-6
                            for a, b in zip(direction, expected, strict=True)
                        )
                        or abs(cosine - verifier.cosine(normal, expected)) > 5e-6
                    ):
                        raise ValueError(
                            "native direction and cosine must preserve the exact original angular source identity"
                        )
                    if pdf == 0.0:
                        if any(weights) or cosine != 0.0:
                            raise ValueError(
                                "unobstructed front-facing native samples cannot discard radiation"
                            )
                    else:
                        if abs(pdf / expected_pmf[emitter["id"]] - 1.0) > 5e-6:
                            raise ValueError(
                                "native emitter probability differs from the unchanged source-derived PMF"
                            )
                        for i in range(4):
                            source = emitter["irradiance_w_m2_nm"][
                                min(offset + i, len(wavelengths) - 1)
                            ]
                            if abs(weights[i] * pdf - source) > 5e-6 * source or (
                                source == 0.0 and weights[i] != 0.0
                            ):
                                raise ValueError(
                                    "native PMF-weighted original spectral knots changed"
                                )
                for i in range(min(4, len(wavelengths) - offset)):
                    sums[offset + i].append(weights[i] * cosine)
        if next(rows, None) is not None:
            raise ValueError("extra original native sample packets rejected")
    means = [math.fsum(values) / receiver["samples"] for values in sums]
    return {
        channel: verifier.spectral_bridge.product_integral(wavelengths, means, weights)
        for channel, weights in normalized["weights"].items()
    }


def verify_observation(work, observation, receiver, normalized, verifier):
    if (
        observation["samples"] != receiver["samples"]
        or type(observation["seed"]) is not int
        or observation["seed"] not in receiver["seeds"]
        or set(observation["components"]) != {"direct", "diffuse"}
        or not isinstance(observation["originals"], list)
        or len(observation["originals"]) != 2
        or any(
            not isinstance(r, dict)
            or set(r) != {"component", "path", "sha256", "bytes"}
            for r in observation["originals"]
        )
        or {r["component"] for r in observation["originals"]} != {"direct", "diffuse"}
    ):
        raise ValueError(
            "approved seed/sample budget and exactly one original per direct/diffuse component required"
        )
    reconstructed = {}
    for record in observation["originals"]:
        path = work / record["path"]
        if (
            record["path"] != f"{record['component']}-{observation['seed']}.csv"
            or path.parent != work
            or path.is_symlink()
            or not path.is_file()
            or path.stat().st_size
            > receiver["samples"]
            * ((len(normalized["wavelengths_nm"]) + 3) // 4)
            * 1024
            + 1024
            or checksum(path) != record["sha256"]
            or path.stat().st_size != record["bytes"]
        ):
            raise ValueError("unchanged bounded original native packet required")
        component = record["component"]
        actual = reconstruct(path, receiver, normalized, component, verifier)
        reconstructed[component] = actual
        reported = observation["components"][component]["native_channels_w_m2"]
        if set(reported) != set(actual) or any(
            not math.isclose(value, reported[key], rel_tol=1e-12, abs_tol=1e-12)
            for key, value in actual.items()
        ):
            raise ValueError(
                "independent original packet cosine/weight integration required"
            )
        checks = verifier.spectral_bridge.verify_channels(
            {**normalized, "reference": normalized["component_references"][component]},
            actual,
            receiver["relative_tolerance"],
        )
        if checks != observation["components"][component]["numerical_verification"]:
            raise ValueError(
                "separate unchanged direct and diffuse numerical gates required"
            )
    combined = {
        name: math.fsum(values[name] for values in reconstructed.values())
        for name in normalized["weights"]
    }
    if (
        combined != observation["native_channels_w_m2"]
        or verifier.spectral_bridge.verify_channels(
            normalized, combined, receiver["relative_tolerance"]
        )
        != observation["numerical_verification"]
        or any(
            not math.isclose(
                observation["exposure_j_m2"][name],
                value * normalized["history_integral_s"],
                rel_tol=1e-14,
                abs_tol=1e-12,
            )
            or not math.isclose(
                observation["native_power_w"][name],
                value * normalized["sensor_area_m2"],
                rel_tol=1e-14,
                abs_tol=1e-12,
            )
            or not math.isclose(
                observation["energy_j"][name],
                value * normalized["history_integral_s"] * normalized["sensor_area_m2"],
                rel_tol=1e-14,
                abs_tol=1e-12,
            )
            for name, value in combined.items()
        )
    ):
        raise ValueError(
            "separate direct/diffuse irradiance, area-dependent power and prescribed optical exposure must independently reconstruct"
        )
    return combined


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    package = parser.add_mutually_exclusive_group(required=True)
    package.add_argument("--runtime", type=Path)
    package.add_argument(
        "--development-site",
        type=Path,
        help="Explicit unqualified extracted-wheel native API diagnostic",
    )
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    sys.path.insert(0, str(repo / "adapters"))
    import atmospheric_spectral as verifier

    native = None
    runtime = None
    closure = None
    paths = []
    environment = {"PYTHONDONTWRITEBYTECODE": "1"}
    if args.runtime:
        runtime = args.runtime.resolve(strict=True)
        if not runtime.is_relative_to("/nix/store") or not runtime.is_file():
            raise ValueError("exact immutable atmospheric spectral runtime required")
        native = json.loads(runtime.read_text())
        if (
            native["schema_version"] != 1
            or native["backend"] != "cpu"
            or native["precision"] != "Float32"
            or native["policy"] != "harbor-cad-atmospheric-spectral-cpu-v1"
        ):
            raise ValueError(
                "strict operation-specific native spectral runtime required"
            )
        closure = Path(native["spectral_closure"]).resolve(strict=True)
        paths = closure.read_text().splitlines()
        if (
            not closure.is_relative_to("/nix/store")
            or len(paths) != len(set(paths))
            or any(
                Path(p).parent != Path("/nix/store") or not Path(p).exists()
                for p in paths
            )
            or str(Path(native["atmospheric_spectral"]).parent.parent) not in paths
        ):
            raise ValueError(
                "distinct complete original atmospheric spectral native closure required"
            )
    else:
        site = args.development_site.resolve(strict=True)
        environment["PYTHONPATH"] = str(site) + ":" + str(repo / "adapters")
    before = service_resources()
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    (root / ".doty-protect").write_text(
        "Manufactured-source native spectral API observations; preserve; no atmospheric runtime qualification.\n"
    )
    records = []
    for name, atmosphere, receiver, original in reference_cases(repo):
        normalized = verifier.normalize_source(atmosphere, receiver, original)
        inputs = root / ("inputs-" + name)
        inputs.mkdir(mode=0o700)
        field = inputs / "atmosphere-original.txt"
        field.write_text(original)
        spec = {
            "schema_version": 1,
            "atmosphere": atmosphere,
            "receiver": receiver,
            "original_path": "/inputs/atmosphere-original.txt",
            "original_sha256": checksum(field),
        }
        request = inputs / "request.json"
        request.write_text(json.dumps(spec, allow_nan=False))
        work = root / name
        work.mkdir(mode=0o700)
        if native:
            command = [
                native["bwrap"],
                "--die-with-parent",
                "--new-session",
                "--unshare-all",
                "--dir",
                "/nix/store",
            ]
            for path in paths:
                command.extend(["--ro-bind", path, path])
            command.extend(
                [
                    "--ro-bind",
                    str(closure),
                    "/spectral-runtime-closure.txt",
                    "--proc",
                    "/proc",
                    "--dev",
                    "/dev",
                    "--tmpfs",
                    "/tmp",
                    "--tmpfs",
                    "/home",
                    "--dir",
                    "/home/worker",
                    "--ro-bind",
                    str(inputs),
                    "/inputs",
                    "--bind",
                    str(work),
                    "/work",
                    "--chdir",
                    "/work",
                    "--clearenv",
                    "--setenv",
                    "HOME",
                    "/home/worker",
                    "--setenv",
                    "LC_ALL",
                    "C",
                    "--setenv",
                    "HARBOR_CAD_ATMOSPHERIC_SPECTRAL_POLICY",
                    native["policy"],
                    "--setenv",
                    "HARBOR_CAD_HOST_NETNS",
                    os.readlink("/proc/self/ns/net"),
                    native["atmospheric_spectral"],
                    "reference",
                    "/inputs/request.json",
                ]
            )
        else:
            code = "import json,pathlib;import atmospheric_spectral as a;r=json.loads(pathlib.Path(__import__('sys').argv[1]).read_text());text=pathlib.Path(__import__('sys').argv[2]).read_bytes().decode();report=a.execute(r['atmosphere'],r['receiver'],text,pathlib.Path.cwd());pathlib.Path('atmospheric-spectral-receipt.json').write_text(json.dumps(report,allow_nan=False,indent=2))"
            command = [
                str(Path(sys.executable).resolve(strict=True)),
                "-B",
                "-c",
                code,
                str(request),
                str(field),
            ]
        (root / (name + "-command.json")).write_text(json.dumps(command, indent=2))
        with (root / (name + ".log")).open("xb") as log:
            process = subprocess.run(
                command,
                cwd=work,
                env=environment,
                stdout=log,
                stderr=subprocess.STDOUT,
                timeout=600,
                check=False,
            )
        (root / (name + "-exit.txt")).write_text(str(process.returncode) + "\n")
        if process.returncode:
            raise RuntimeError(
                f"native atmospheric spectral gate failed; originals retained at {work}"
            )
        receipt = json.loads((work / "atmospheric-spectral-receipt.json").read_text())
        if (
            receipt["source_input"] != atmosphere
            or receipt["receiver_input"] != receiver
            or receipt["original_atmosphere_sha256"] != checksum(field)
            or receipt["executed"] is not True
            or receipt["software_fallback"] is not False
            or receipt["reference"] != normalized["reference"]
            or receipt["component_references"] != normalized["component_references"]
            or [o["seed"] for o in receipt["observations"]] != receiver["seeds"]
            or receipt["transfer_relative_conservation_error"] > 1e-10
        ):
            raise ValueError(
                "unchanged native scientific inputs, full direct/diffuse originals and seeds required"
            )
        if native and (
            receipt["request_sha256"] != checksum(request)
            or receipt["sandbox"]["policy"] != native["policy"]
            or len(receipt["sandbox"]["checks"]) != 9
            or not all(receipt["sandbox"]["checks"].values())
        ):
            raise ValueError(
                "exact complete measured original-readonly sandbox canaries required"
            )
        derived = receipt["derived_emitter_sources"]
        derived_path = work / derived["path"]
        if (
            derived_path.parent != work
            or derived_path.is_symlink()
            or checksum(derived_path) != derived["sha256"]
            or derived_path.stat().st_size != derived["bytes"]
        ):
            raise ValueError(
                "unchanged derived native angular emitter source manifest required"
            )
        mapped = json.loads(derived_path.read_text())
        if (
            mapped["direct"] != normalized["direct_emitter"]
            or mapped["diffuse"] != normalized["diffuse_emitters"]
            or mapped["wavelengths_nm"] != normalized["wavelengths_nm"]
            or any(
                not math.isfinite(area)
                or abs(area / normalized["sensor_area_m2"] - 1.0) > 5e-6
                for area in receipt["native_sensor_area_m2"].values()
            )
        ):
            raise ValueError(
                "complete unchanged original-to-emitter mapping and native SI area required"
            )
        for observation in receipt["observations"]:
            verify_observation(work, observation, receiver, normalized, verifier)
        records.append(
            {
                "case": name,
                "request": spec,
                "receipt": receipt,
                "original_files_sha256": {
                    p.name: checksum(p) for p in work.iterdir() if p.is_file()
                },
                "exit_code": process.returncode,
            }
        )
    report = {
        "schema_version": 1,
        "scope": "manufactured angular-reference native spectral transport only; no libRadtran runtime, registered-source worker, convergence, GPU or physical qualification",
        "package_qualification": "exact immutable renderer and operation sandbox"
        if runtime
        else "unqualified extracted-wheel diagnostic",
        "runtime": str(runtime) if runtime else None,
        "runtime_sha256": checksum(runtime) if runtime else None,
        "adapter_source_sha256": checksum(repo / "adapters/atmospheric_spectral.py"),
        "results": records,
        "service_resources_before": before,
        "service_resources_after": service_resources(),
        "physical_validation": "unqualified",
    }
    (root / "verification.json").write_text(
        json.dumps(report, allow_nan=False, indent=2)
    )
    print(
        json.dumps(
            {
                "results": len(records),
                "report_sha256": checksum(root / "verification.json"),
            }
        )
    )


if __name__ == "__main__":
    main()
