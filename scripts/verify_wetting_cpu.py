"""Exact native wetting phase-mass/contact-angle reference under a CPU-only closure sandbox."""

import argparse
import hashlib
import importlib.util
import json
import math
import os
import subprocess
from itertools import pairwise
from pathlib import Path

from verify_openlb_hip import service_resources


def checksum(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def request(angle, n):
    # Hold physical duration fixed as dt scales with dx². The initial cap
    # requires this settling budget before comparing equilibrium angles.
    steps = 24000 * n * n // (24 * 24)
    return {
        "schema_version": 1,
        "synthetic": True,
        "backend": "cpu",
        "formulation": "well_balanced_contact_angle_2d",
        "diameter_m": 48e-6,
        "resolution": n,
        "interface_width_m": 6e-6,
        "density_liquid_kg_m3": 1000.0,
        "density_vapor_kg_m3": 1000.0,
        "viscosity_liquid_m2_s": 1e-6,
        "viscosity_vapor_m2_s": 1e-6,
        "surface_tension_n_m": 1e-4,
        "contact_angle_deg": angle,
        "phase_relaxation_time": 1.0,
        "steps": steps,
        "observation_steps": [0, steps // 2, 3 * steps // 4, steps],
        "mass_tolerance": 1e-3,
        "angle_tolerance_deg": 5.0,
        "material_provenance": "controlled synthetic equal-density/equal-viscosity diffuse-interface reference; not water-air properties",
        "boundary_provenance": "uniform prescribed planar wetting contact angle; periodic x, impermeable y-normal walls, no gravity or inlet",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("runtime", "output"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    args = parser.parse_args()
    runtime = args.runtime.resolve(strict=True)
    if not runtime.is_relative_to("/nix/store"):
        raise ValueError("exact immutable packaged CPU wetting runtime required")
    native = json.loads(runtime.read_text())
    if native["backend"] != "cpu" or native["schema_version"] != 1:
        raise ValueError("explicit CPU runtime required")
    path = Path(__file__).resolve().parents[1] / "adapters/wetting_reference.py"
    spec = importlib.util.spec_from_file_location("wetting", path)
    bridge = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(bridge)
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    before = service_resources()
    closure = Path(native["wetting_closure"]).resolve(strict=True)
    mounts = closure.read_text().splitlines()
    if len(mounts) != len(set(mounts)) or any(
        not Path(p).is_relative_to("/nix/store")
        or Path(p).parent != Path("/nix/store")
        or not Path(p).exists()
        for p in mounts
    ):
        raise ValueError("exact distinct operation closure paths required")

    def run(key, descriptor):
        inputs, output = root / f"inputs-{key}", root / key
        inputs.mkdir(mode=0o700)
        output.mkdir(mode=0o700)
        request_path = inputs / "request.json"
        request_path.write_text(json.dumps(descriptor, allow_nan=False, indent=2))
        argv = [
            native["bwrap"],
            "--die-with-parent",
            "--new-session",
            "--unshare-all",
            "--dir",
            "/nix",
            "--dir",
            "/nix/store",
        ]
        for mount in mounts:
            argv.extend(["--ro-bind", mount, mount])
        argv += [
            "--ro-bind",
            str(closure),
            "/wetting-runtime-closure.txt",
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
            str(output),
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
            "HARBOR_CAD_WETTING_POLICY",
            bridge.POLICY,
            "--setenv",
            "HARBOR_CAD_HOST_NETNS",
            os.readlink("/proc/self/ns/net"),
            native["wetting"],
            "reference",
            "/inputs/request.json",
        ]
        process = subprocess.run(argv, capture_output=True, timeout=500, check=False)
        log = root / f"launch-{key}.log"
        log.write_bytes(process.stdout + process.stderr)
        return process, output, argv, log, request_path

    results = []
    for angle in (90, 100):
        for n in (24, 36, 48):
            descriptor = request(angle, n)
            bridge.validate(descriptor)
            process, output, argv, log, request_path = run(
                f"angle{angle}-n{n}", descriptor
            )
            if process.returncode:
                raise RuntimeError(
                    f"native wetting gate failed; retain and inspect {output} and {log}"
                )
            receipt = json.loads((output / "verified-wetting-receipt.json").read_text())
            if (
                receipt["request"] != descriptor
                or receipt["request_sha256"] != checksum(request_path)
                or receipt["physical_validation"] != "unqualified"
                or receipt["sandbox"]["policy"] != bridge.POLICY
                or len(receipt["sandbox"]["checks"]) != 8
                or not all(receipt["sandbox"]["checks"].values())
            ):
                raise ValueError("native science/bytes/isolation identity changed")
            independently = []
            dt = bridge.validate(descriptor)["physical_step_s"]
            for field in receipt["independent_fields"]:
                data = (output / field["path"]).read_bytes()
                check = bridge.assess_field(descriptor, data)
                if (
                    check != field["check"]
                    or field["sha256"] != checksum(output / field["path"])
                    or not math.isclose(
                        field["time_s"],
                        field["step"] * dt,
                        rel_tol=1e-12,
                        abs_tol=1e-18,
                    )
                ):
                    raise ValueError(
                        "independent contact contour/phase mass/SI time or raw bytes changed"
                    )
                independently.append({**field, "check": check})
            checked = bridge.verify(descriptor, independently)
            angles = [v["check"]["contact_angle_deg"] for v in independently]
            equilibrium = abs(angles[-1] - angles[-2])
            if (
                checked != receipt["numerical_verification"]
                or not checked["mass_passed"]
                or not checked["angle_passed"]
                or equilibrium > 0.2
            ):
                raise ValueError("mass/angle/reference equilibrium gate failed")
            results.append(
                {
                    "angle": angle,
                    "resolution": n,
                    "request": descriptor,
                    "argv": argv,
                    "log_sha256": checksum(log),
                    "receipt": receipt,
                    "independent_checks": checked,
                    "late_angle_difference_deg": equilibrium,
                }
            )
    refinements = []
    for angle in (90, 100):
        errors = [
            r["independent_checks"]["angle_abs_error_deg"]
            for r in results
            if r["angle"] == angle
        ]
        if any(b >= a for a, b in pairwise(errors)):
            raise ValueError(
                "fixed physical diffuse-interface mesh refinement did not decrease contact-angle error"
            )
        refinements.append(
            {
                "angle": angle,
                "resolutions": [24, 36, 48],
                "angle_errors_deg": errors,
                "physical_time_s": results[0]["receipt"]["snapshots"][-1]["time_s"],
                "passed": True,
            }
        )
    base = request(90, 24)
    rejections = []
    for key, value in (
        ("backend", "hip"),
        ("synthetic", False),
        ("density_vapor_kg_m3", 1.2),
        ("mass_tolerance", 0.1),
        ("angle_tolerance_deg", 6),
        ("interface_width_m", 1e-7),
        ("material_provenance", ""),
        ("boundary_provenance", ""),
        ("observation_steps", [0, 0, base["steps"]]),
        ("resolution", True),
    ):
        process, output, argv, log, _ = run(f"reject-{key}", {**base, key: value})
        if not process.returncode or list(output.iterdir()):
            raise ValueError(f"unsupported wetting input ran or published: {key}")
        rejections.append(
            {
                "field": key,
                "exit_code": process.returncode,
                "argv": argv,
                "log_sha256": checksum(log),
            }
        )
    report = {
        "schema_version": 1,
        "runtime": str(runtime),
        "runtime_sha256": checksum(runtime),
        "results": results,
        "refinements": refinements,
        "rejections": rejections,
        "service_resources_before": before,
        "service_resources_after": service_resources(),
        "physical_validation": "unqualified",
        "scope": "static synthetic CPU 2D planar diffuse-interface wetting mass/contact angle; no inlet spray, ingress, vapor flux or transient phase-change conclusion",
    }
    (root / "verification.json").write_text(
        json.dumps(report, indent=2, allow_nan=False)
    )
    print(
        json.dumps(
            {
                "report_sha256": checksum(root / "verification.json"),
                "results": len(results),
                "rejections": len(rejections),
                "refinements": refinements,
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
