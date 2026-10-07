"""Exact native molecular UV originals, vacuum law and separate refinement gates."""

import argparse
import copy
import importlib.util
import json
import math
import os
import subprocess
from itertools import pairwise
from pathlib import Path

from verify_openlb_hip import service_resources
from verify_spectral_cpu import checksum


def relative_l2(actual, reference):
    if (
        len(actual) != len(reference)
        or not actual
        or any(not math.isfinite(v) for v in (*actual, *reference))
    ):
        raise ValueError(
            "complete finite comparable native atmospheric values required"
        )
    norm = math.sqrt(math.fsum(v * v for v in reference))
    if norm == 0:
        if any(actual):
            raise ValueError(
                "zero native atmospheric reference cannot normalize nonzero fields"
            )
        return 0.0
    return (
        math.sqrt(
            math.fsum((a - b) ** 2 for a, b in zip(actual, reference, strict=True))
        )
        / norm
    )


def flatten_flux(record):
    return [
        v
        for key in (
            "direct_horizontal_w_m2_nm",
            "diffuse_downward_w_m2_nm",
            "diffuse_upward_w_m2_nm",
        )
        for v in record["observations"][key]
    ]


def spectral_integrals(record):
    wavelengths = record["prepared"]["wavelengths_nm"]
    return [
        math.fsum(
            (b - a) * (values[i] + values[i + 1]) / 2
            for i, (a, b) in enumerate(pairwise(wavelengths))
        )
        for key in (
            "direct_horizontal_w_m2_nm",
            "diffuse_downward_w_m2_nm",
            "diffuse_upward_w_m2_nm",
        )
        for values in [record["observations"][key]]
    ]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("executable", "runtime", "output"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    args = parser.parse_args()
    binary = args.executable.resolve(strict=True)
    descriptor = args.runtime.resolve(strict=True)
    if any(
        not p.is_relative_to("/nix/store") or not p.is_file()
        for p in (binary, descriptor)
    ):
        raise ValueError("exact immutable atmospheric planner and runtime required")
    runtime = json.loads(descriptor.read_text())
    if (
        set(runtime)
        != {
            "schema_version",
            "bwrap",
            "atmosphere",
            "atmosphere_closure",
            "backend",
            "solver",
            "precision",
            "source_sha256",
            "qualification",
        }
        or runtime["schema_version"] != 1
        or runtime["backend"] != "cpu"
        or runtime["solver"] != "disort"
        or runtime["precision"] != "Float32"
    ):
        raise ValueError("strict exact native CPU DISORT Float32 runtime required")
    for key in ("bwrap", "atmosphere", "atmosphere_closure"):
        path = Path(runtime[key]).resolve(strict=True)
        if not path.is_relative_to("/nix/store") or not path.is_file():
            raise ValueError(
                "immutable operation-specific native runtime files required"
            )
    closure = Path(runtime["atmosphere_closure"])
    paths = closure.read_text().splitlines()
    if (
        not paths
        or len(paths) > 4096
        or len(paths) != len(set(paths))
        or any(
            Path(p).parent != Path("/nix/store")
            or Path(p).resolve(strict=True) != Path(p)
            or not Path(p).exists()
            for p in paths
        )
        or str(Path(runtime["atmosphere"]).parent.parent) not in paths
    ):
        raise ValueError(
            "complete bounded canonical immutable atmosphere-only operation closure required"
        )
    repo = Path(__file__).resolve().parents[1]
    module = importlib.util.spec_from_file_location(
        "independent_atmosphere_originals", repo / "adapters/atmosphere_reference.py"
    )
    bridge = importlib.util.module_from_spec(module)
    module.loader.exec_module(bridge)
    if runtime["source_sha256"] != bridge.SOURCE_SHA256:
        raise ValueError("matching exact official libRadtran source pin required")
    before = service_resources()
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    (root / ".doty-protect").write_text(
        "Exact native atmospheric original fields, resource metrics and failed attempts; preserve.\n"
    )
    base = json.loads((repo / "examples/atmosphere-reference.json").read_text())
    records = {}

    def run(name, spec, expect_rejection=False):
        inputs = root / f"input-{name}"
        inputs.mkdir(mode=0o700)
        request = inputs / "request.json"
        request.write_text(json.dumps(spec, allow_nan=False))
        planned = subprocess.run(
            [str(binary), "case", "validate-atmospheric-reference", str(request)],
            capture_output=True,
            check=False,
            timeout=30,
        )
        (root / f"planner-{name}.json").write_bytes(planned.stdout)
        (root / f"planner-{name}.log").write_bytes(planned.stderr)
        work = root / name
        work.mkdir(mode=0o700)
        command = [
            runtime["bwrap"],
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
                "/atmosphere-runtime-closure.txt",
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
                "HARBOR_CAD_ATMOSPHERE_POLICY",
                bridge.POLICY,
                "--setenv",
                "HARBOR_CAD_HOST_NETNS",
                os.readlink("/proc/self/ns/net"),
                runtime["atmosphere"],
                "reference",
                "/inputs/request.json",
            ]
        )
        with (root / f"launch-{name}.log").open("xb") as log:
            process = subprocess.run(
                command,
                cwd=work,
                stdout=log,
                stderr=subprocess.STDOUT,
                timeout=310,
                check=False,
            )
        (root / f"exit-{name}.txt").write_text(str(process.returncode))
        if expect_rejection:
            if (
                planned.returncode == 0
                or process.returncode == 0
                or list(work.iterdir())
            ):
                raise ValueError(
                    "strict invalid atmospheric inputs must reject before native fields are written"
                )
            return {
                "case": name,
                "planner_exit": planned.returncode,
                "adapter_exit": process.returncode,
                "log_sha256": checksum(root / f"launch-{name}.log"),
                "pre_output": True,
            }
        if planned.returncode or process.returncode:
            raise RuntimeError(
                f"exact native atmosphere failed; original fields/logs preserved at {work}"
            )
        prepared = json.loads(planned.stdout)
        normalized = bridge.normalize(spec)
        for key, value in normalized.items():
            if isinstance(value, (list, float)):
                a = value if isinstance(value, list) else [value]
                b = prepared[key] if isinstance(value, list) else [prepared[key]]
                if len(a) != len(b) or any(
                    not math.isclose(x, y, rel_tol=2e-14, abs_tol=1e-15)
                    for x, y in zip(a, b, strict=True)
                ):
                    raise ValueError(
                        "independent SI/source/angular preparation differs"
                    )
            elif value != prepared[key]:
                raise ValueError(
                    "independent atmospheric formulation/preparation differs"
                )
        receipt = json.loads((work / "atmosphere-receipt.json").read_text())
        if (
            receipt["input"] != spec
            or receipt["request_sha256"] != checksum(request)
            or receipt["source_sha256"] != bridge.SOURCE_SHA256
            or receipt["profile_sha256"] != bridge.PROFILE_SHA256[spec["profile"]]
            or not Path(receipt["native_executable"])
            .resolve(strict=True)
            .is_relative_to("/nix/store")
            or checksum(Path(receipt["native_executable"]))
            != receipt["native_executable_sha256"]
            or receipt["precision"] != "Float32"
            or receipt["reduction_precision"] != "Float64"
            or receipt["executed"] is not True
            or receipt["software_fallback"] is not False
            or receipt["prepared"] != normalized
            or receipt["sandbox"]["policy"] != bridge.POLICY
            or len(receipt["sandbox"]["checks"]) != 8
            or not all(receipt["sandbox"]["checks"].values())
        ):
            raise ValueError(
                "exact original native identity, precision, closure-only isolation and unchanged input required"
            )
        original = work / "uvspec-original.txt"
        if receipt["original"] != {
            "path": original.name,
            "bytes": original.stat().st_size,
            "sha256": checksum(original),
        }:
            raise ValueError("original native atmospheric text identity changed")
        decoded = bridge.parse_original(original.read_text(), spec, normalized)
        summary = {k: v for k, v in decoded.items() if k != "radiance_w_m2_sr_nm"}
        if summary != receipt["observations"] or receipt["angular_fields"] != {
            "path": "uvspec-original.txt",
            "shape": [
                len(normalized["wavelengths_nm"]),
                2 * spec["mu_bins"],
                spec["phi_bins"],
            ],
            "association": "wavelength_propagation_solid_angle_sample",
            "units": "W/(m2*sr*nm)",
            "precision": "Float32",
            "columns_start": 4,
            "ordering": "umu-major phi-minor",
        }:
            raise ValueError(
                "original native angular/flux reconstruction differs from receipt"
            )
        # Reconstruct the complete retained angular sphere independently of the
        # adapter's row decoder, without replacing it by isotropic radiance.
        for row in original.read_text().splitlines():
            values = list(map(float, row.split()))
            for sign, flux in ((-1, values[2]), (1, values[3])):
                total = math.fsum(
                    values[4 + i * spec["phi_bins"] + j]
                    * abs(mu)
                    * normalized["angular_cell_solid_angle_sr"]
                    for i, mu in enumerate(normalized["umu"])
                    if mu * sign > 0
                    for j in range(spec["phi_bins"])
                )
                if flux == 0:
                    if total != 0:
                        raise ValueError(
                            "zero diffuse flux has nonzero original angular power"
                        )
                elif abs(total / flux - 1) > spec["relative_tolerance"]:
                    raise ValueError("independent original angular flux gate failed")
        hashes = {p.name: checksum(p) for p in work.iterdir() if p.is_file()}
        record = {
            "case": name,
            "request": spec,
            "prepared": normalized,
            "observations": summary,
            "original_files_sha256": hashes,
            "receipt_sha256": checksum(work / "atmosphere-receipt.json"),
        }
        records[name] = record
        return record

    for zenith, azimuth in ((0, 0), (30, 0), (70, 90)):
        spec = copy.deepcopy(base)
        spec.update(
            model="transparent_reference",
            solar_zenith_deg=zenith,
            solar_azimuth_deg=azimuth,
        )
        run(f"transparent-z{zenith}-a{azimuth}", spec)
    for streams in (16, 32, 64):
        spec = copy.deepcopy(base)
        spec.update(streams=streams, mu_bins=64, phi_bins=64)
        run(f"clear-streams{streams}", spec)
    for bins in (16, 32):
        spec = copy.deepcopy(base)
        spec.update(streams=64, mu_bins=bins, phi_bins=bins)
        run(f"clear-angular{bins}", spec)
    reflected = copy.deepcopy(base)
    reflected.update(streams=64, mu_bins=64, phi_bins=64, albedo=0.6)
    run("clear-rho06", reflected)
    for step in (4, 2, 1):
        spec = copy.deepcopy(base)
        spec.update(streams=64, mu_bins=64, phi_bins=64)
        wavelengths = list(range(300, 361, step))
        spec["wavelengths"] = [{"value": w, "unit": "nm"} for w in wavelengths]
        spec["toa_irradiance"] = [
            {
                "value": 1 + (w - 280) / 40 if w <= 320 else 2 + (w - 320) / 80,
                "unit": "W/(m2*nm)",
            }
            for w in wavelengths
        ]
        run(f"clear-wavelength{step}", spec)
    fine = records["clear-streams64"]
    stream_errors = [
        relative_l2(flatten_flux(records[f"clear-streams{s}"]), flatten_flux(fine))
        for s in (16, 32)
    ]

    # Angular shape convergence compares coarse native cell values with the
    # solid-angle mean of corresponding fine native cells on the same sphere.
    def angular_original(record):
        original = root / record["case"] / "uvspec-original.txt"
        if checksum(original) != record["original_files_sha256"][original.name]:
            raise ValueError("refinement original native angular field changed")
        return [
            list(map(float, line.split()))[4:]
            for line in original.read_text().splitlines()
        ]

    angular_errors = []
    fine_angular = angular_original(fine)
    for bins in (16, 32):
        factor = 64 // bins
        coarse = angular_original(records[f"clear-angular{bins}"])
        reduced = []
        for row in fine_angular:
            reduced.extend(
                math.fsum(
                    row[(i * factor + di) * 64 + j * factor + dj]
                    for di in range(factor)
                    for dj in range(factor)
                )
                / (factor * factor)
                for i in range(2 * bins)
                for j in range(bins)
            )
        angular_errors.append(relative_l2([v for row in coarse for v in row], reduced))
    spectral_errors = [
        relative_l2(
            spectral_integrals(records[f"clear-wavelength{s}"]),
            spectral_integrals(records["clear-wavelength1"]),
        )
        for s in (4, 2)
    ]
    refinements = {}
    for name, errors in (
        ("streams", stream_errors),
        ("angular_shape", angular_errors),
        ("wavelength_integrals", spectral_errors),
    ):
        passed = errors[1] <= base["relative_tolerance"] and (
            errors[1] < errors[0] or max(errors) <= 1e-7
        )
        refinements[name] = {
            "relative_l2_errors_against_finest": errors,
            "tolerance": base["relative_tolerance"],
            "passed": passed,
            "roundoff_plateau": max(errors) <= 1e-7,
            "scope": "separate same-model refinement; no physical validation",
        }
    (root / "refinements.json").write_text(
        json.dumps(refinements, indent=2, allow_nan=False)
    )
    if not all(r["passed"] for r in refinements.values()):
        raise ValueError(
            "unchanged separate atmospheric stream/angular/wavelength convergence gates failed; originals retained"
        )
    rejections = []
    for name, field, value in (
        ("unsupported-backend", "backend", "hip"),
        ("profile-traversal", "profile", "../../credentials"),
        ("weak-gate", "relative_tolerance", 0.1),
        ("grazing-sun", "solar_zenith_deg", 90),
        ("unbounded-angles", "mu_bins", 100000),
        ("implicit-sky", "diffuse_isotropic", True),
        ("native-deck-injection", "uvspec_input", "include /etc/shadow"),
        (
            "sub-native-resolution",
            "wavelengths",
            [
                {"value": 280.0001, "unit": "nm"},
                {"value": 320, "unit": "nm"},
                {"value": 400, "unit": "nm"},
            ],
        ),
    ):
        spec = copy.deepcopy(base)
        spec[field] = value
        rejections.append(run(name, spec, True))
    report = {
        "schema_version": 1,
        "binary": str(binary),
        "binary_sha256": checksum(binary),
        "runtime": str(descriptor),
        "runtime_sha256": checksum(descriptor),
        "source_sha256": bridge.SOURCE_SHA256,
        "results": list(records.values()),
        "rejections": rejections,
        "refinements": refinements,
        "service_resources_before": before,
        "service_resources_after": service_resources(),
        "runtime_qualification": "exact immutable standalone synthetic CPU atmosphere; worker and spectral transport unqualified",
        "physical_validation": "unqualified",
    }
    (root / "verification.json").write_text(
        json.dumps(report, indent=2, allow_nan=False)
    )
    print(
        json.dumps(
            {
                "cases": len(records),
                "rejections": len(rejections),
                "refinements": refinements,
                "report_sha256": checksum(root / "verification.json"),
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
