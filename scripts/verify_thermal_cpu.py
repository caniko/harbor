"""Bounded native transient cold/heater/convection references and separate refinement sweeps."""

import argparse
import hashlib
import json
import subprocess
from itertools import pairwise
from pathlib import Path

from verify_openlb_hip import service_resources


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    resources = service_resources()
    runtime = args.runtime.resolve(strict=True)
    if not runtime.is_relative_to("/nix/store") or not runtime.is_file():
        raise ValueError("exact immutable thermal reference runtime required")
    native = json.loads(runtime.read_text())
    if native["schema_version"] != 1 or native["backend"] != "cpu":
        raise ValueError("explicit CPU transient reference runtime required")
    binary = Path(native["thermal"]).resolve(strict=True)
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    inputs = root / "inputs"
    inputs.mkdir(mode=0o700)
    base = {
        "schema_version": 1,
        "synthetic": True,
        "backend": "cpu",
        "formulation": "plane_wall_robin",
        "size_m": [0.02, 0.01, 0.01],
        "resolution": 4,
        "geometry_tolerance_m": 1e-6,
        "initial_temperature_k": 293.15,
        "density_kg_m3": 7800.0,
        "specific_heat_j_kg_k": 500.0,
        "conductivity_w_m_k": 20.0,
        "material_temperature_domain_k": [240.0, 320.0],
        "convection_w_m2_k": 200.0,
        "duration_s": 120.0,
        "max_step_s": 1.0,
        "observation_times_s": [10.0, 60.0, 120.0],
        "ambient_history": [[0.0, 253.15], [60.0, 253.15], [120.0, 273.15]],
        "heater_history": [[0.0, 0.0], [60.0, 0.0], [120.0, 1.0]],
        "numerical_tolerance": 0.02,
        "energy_tolerance": 0.02,
        "geometry_provenance": "synthetic reference box",
        "material_provenance": "synthetic constant-property steel-like solid",
        "history_provenance": "prescribed synthetic cold soak then heater ramp",
        "convection_provenance": "prescribed synthetic h; no velocity conversion",
        "moisture_risk": {"assessment": "missing", "reason": "no humidity supplied"},
    }

    def invoke(label, spec):
        path = inputs / f"{label}.json"
        path.write_text(json.dumps(spec, allow_nan=False, indent=2))
        output = root / label
        output.mkdir(mode=0o700)
        command = [
            native["bwrap"],
            "--die-with-parent",
            "--new-session",
            "--unshare-all",
            "--ro-bind",
            "/nix/store",
            "/nix/store",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--tmpfs",
            "/tmp",
            "--tmpfs",
            "/home",
            "--dir",
            "/home/native",
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
            "/home/native",
            "--setenv",
            "LC_ALL",
            "C",
            str(binary),
            "run",
            f"/inputs/{path.name}",
        ]
        result = subprocess.run(
            command, capture_output=True, text=True, timeout=200, check=False
        )
        (output / "process.log").write_text(result.stdout + result.stderr)
        return output, result, command

    results = []
    for name, refinements in [
        ("adiabatic_heater", [(2, 2.0), (4, 1.0), (8, 0.5)]),
        ("cold_restart_robin", [(2, 0.5), (4, 0.5), (8, 0.5), (8, 1.0), (8, 2.0)]),
    ]:
        for resolution, step in refinements:
            spec = {**base, "resolution": resolution, "max_step_s": step}
            if name == "adiabatic_heater":
                spec.update(
                    convection_w_m2_k=0.0,
                    initial_temperature_k=253.15,
                    ambient_history=[[0.0, 253.15], [120.0, 253.15]],
                )
            label = f"{name}-n{resolution}-dt{step:g}"
            output, process, command = invoke(label, spec)
            if process.returncode:
                raise RuntimeError(
                    f"native thermal gate failed: {output}\n{process.stderr[-4000:]}"
                )
            receipt = json.loads((output / "thermal-receipt.json").read_text())
            if (
                receipt["backend"] != "cpu"
                or receipt["factorization"] != "SPOOLES"
                or not receipt["executed"]
                or receipt["software_fallback"]
                or receipt["calculix_version"] != "2.23"
                or receipt["gmsh_version"] != "4.15.2"
            ):
                raise ValueError("exact native CPU transient stack did not execute")
            if (
                receipt["request_sha256"]
                != hashlib.sha256((inputs / f"{label}.json").read_bytes()).hexdigest()
                or receipt["native_field_sha256"]
                != hashlib.sha256((output / "reference.dat").read_bytes()).hexdigest()
                or receipt["mesh_sha256"]
                != hashlib.sha256((output / "mesh.json").read_bytes()).hexdigest()
            ):
                raise ValueError("native request/field/mesh bytes changed")
            fields = json.loads((output / "thermal-fields.json").read_text())
            if (
                fields["association"] != "point"
                or fields["unit"] != "K"
                or [v["requested_s"] for v in fields["times"]]
                != spec["observation_times_s"]
            ):
                raise ValueError(
                    "native field association, units or retained physical times changed"
                )
            if (
                receipt["physical_validation"] != "unqualified"
                or receipt["moisture_risk"] != base["moisture_risk"]
            ):
                raise ValueError(
                    "execution promoted unknown moisture/physical conclusions"
                )
            records = {}
            for path in sorted(output.iterdir()):
                if (
                    path.is_symlink()
                    or not path.is_file()
                    or path.stat().st_size > 64 * 1024**2
                ):
                    raise ValueError(
                        "bounded closed regular thermal output records required"
                    )
                records[path.name] = {
                    "bytes": path.stat().st_size,
                    "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                }
            if sum(v["bytes"] for v in records.values()) > 256 * 1024**2:
                raise ValueError("native thermal output budget exhausted")
            results.append(
                {
                    "recipe": name,
                    "request": spec,
                    "argv": command,
                    "receipt": receipt,
                    "artifacts": records,
                }
            )
    conv = {
        (v["request"]["resolution"], v["request"]["max_step_s"]): v["receipt"][
            "numerical_verification"
        ]["temperature"]["normalized_max_abs_error"]
        for v in results
        if v["recipe"] == "cold_restart_robin"
    }
    spatial = [conv[n, 0.5] for n in (2, 4, 8)]
    temporal = [conv[8, dt] for dt in (2.0, 1.0, 0.5)]
    if any(b > a for errors in (spatial, temporal) for a, b in pairwise(errors)):
        raise ValueError(
            f"independent spatial/temporal reference error does not decrease: {spatial}, {temporal}"
        )
    rejections = []
    for key, value in [
        ("backend", "hip"),
        ("synthetic", False),
        ("convection_provenance", ""),
        ("density_kg_m3", 0.0),
        ("material_temperature_domain_k", [270.0, 320.0]),
        ("max_step_s", 1e-10),
        ("observation_times_s", [10.0, 120.0, 60.0]),
        ("numerical_tolerance", 0.1),
        ("moisture_risk", None),
    ]:
        output, process, command = invoke(f"reject-{key}", {**base, key: value})
        if (
            not process.returncode
            or (output / "mesh.json").exists()
            or (output / "thermal-receipt.json").exists()
        ):
            raise ValueError("unsupported thermal inputs ran or were silently replaced")
        rejections.append(
            {
                "field": key,
                "exit_code": process.returncode,
                "argv": command,
                "reason": process.stderr[-2000:],
            }
        )
    report = {
        "schema_version": 1,
        "scope": "synthetic constant-property planar CPU transient thermal history; no worker/CAD/contact/GPU/physical qualification",
        "runtime": str(runtime),
        "runtime_sha256": hashlib.sha256(runtime.read_bytes()).hexdigest(),
        "adapter": str(binary),
        "service_resources_before": resources,
        "service_resources_after": service_resources(),
        "results": results,
        "rejections": rejections,
        "spatial_errors_n2_n4_n8": spatial,
        "temporal_errors_dt2_dt1_dt05": temporal,
        "physical_validation": "unqualified",
    }
    (root / "verification.json").write_text(
        json.dumps(report, allow_nan=False, indent=2)
    )
    print(json.dumps(report, allow_nan=False, indent=2))


if __name__ == "__main__":
    main()
