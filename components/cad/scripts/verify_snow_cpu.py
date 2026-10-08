"""Prescribed snow resistance through exact native thermal grids and refinements."""

import argparse
import copy
import json
import math
import os
import subprocess
from itertools import pairwise
from pathlib import Path

from verify_contact_cpu import load
from verify_openlb_hip import service_resources
from verify_thermal_cpu import temporal_self_convergence
from verify_wetting_cpu import checksum


def thermal_runtime_descriptor(native, *, worker_runtime):
    if worker_runtime:
        if native.get("openlb_backend") != "cpu" or any(
            value is not None
            for key, value in native.items()
            if key not in ("bwrap", "thermal", "thermal_closure", "openlb_backend")
        ):
            raise ValueError("exact operation-only CPU thermal worker runtime required")
    elif native.get("schema_version") != 1 or native.get("backend") != "cpu":
        raise ValueError("exact CPU thermal reference required")
    if not native.get("thermal_closure"):
        raise ValueError(
            "native thermal reference descriptor lacks its operation-specific closure; realize the current runtime-thermal-cpu package"
        )
    return native


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("executable", "output"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    descriptor = parser.add_mutually_exclusive_group(required=True)
    descriptor.add_argument(
        "--runtime", type=Path, help="Exact standalone thermal reference descriptor"
    )
    descriptor.add_argument(
        "--worker-runtime",
        type=Path,
        help="Exact existing operation-only thermal worker descriptor and its native closure",
    )
    parser.add_argument(
        "--development-planner",
        action="store_true",
        help="Retain an explicitly unqualified source-built planner diagnostic",
    )
    args = parser.parse_args()
    binary, runtime = (
        p.resolve(strict=True)
        for p in (args.executable, args.runtime or args.worker_runtime)
    )
    if (
        not runtime.is_relative_to("/nix/store")
        or not runtime.is_file()
        or not binary.is_file()
    ):
        raise ValueError(
            "exact immutable native thermal runtime and regular Rust planner required"
        )
    if not args.development_planner and not binary.is_relative_to("/nix/store"):
        raise ValueError("exact immutable production planner required")
    native = thermal_runtime_descriptor(
        json.loads(runtime.read_text()), worker_runtime=args.worker_runtime is not None
    )
    closure = Path(native["thermal_closure"]).resolve(strict=True)
    paths = closure.read_text().splitlines()
    if (
        not paths
        or len(paths) != len(set(paths))
        or any(
            Path(p).parent != Path("/nix/store") or not Path(p).exists() for p in paths
        )
        or Path(native["thermal"]).parent.parent.as_posix() not in paths
    ):
        raise ValueError(
            "distinct complete operation-specific thermal closure required"
        )
    repo = Path(__file__).resolve().parents[1]
    bridge = load("snow_thermal_verification", repo / "adapters/thermal_history.py")
    fem = load("snow_fem_verification", repo / "adapters/fem_reference.py")
    base = json.loads((repo / "examples/snow-reference.json").read_text())
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    (root / ".doty-protect").write_text(
        "Active prescribed snow native reference originals and failures; preserve.\n"
    )
    before = service_resources()
    results, rejections = [], []

    def prepare(spec):
        process = subprocess.run(
            [str(binary), "case", "plan-snow-reference", "/dev/stdin"],
            input=json.dumps(spec, allow_nan=False).encode(),
            capture_output=True,
            timeout=30,
            check=False,
        )
        if process.returncode:
            raise RuntimeError(process.stdout.decode())
        planned = json.loads(process.stdout)
        boundary = planned["snow_boundary"]
        d, k, h = (
            spec["snow"]["thickness"]["value"] / 1000,
            spec["snow"]["conductivity"]["value"],
            spec["thermal"]["convection_w_m2_k"],
        )
        expected = 1 / (1 / h + d / k)
        if (
            boundary["executed"] is not False
            or boundary["input"] != spec
            or boundary["native"] != planned["plan"]["thermal"]
            or not math.isclose(
                boundary["effective_convection_w_m2_k"], expected, rel_tol=1e-14
            )
        ):
            raise ValueError(
                "original prescribed snow and independent SI series-resistance preparation required"
            )
        return planned

    def run(name, spec, planned):
        inputs, work = root / f"input-{name}", root / name
        inputs.mkdir(mode=0o700)
        work.mkdir(mode=0o700)
        request = inputs / "request.json"
        request.write_text(json.dumps(spec, allow_nan=False, separators=(",", ":")))
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
                "/thermal-runtime-closure.txt",
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
                "HARBOR_CAD_THERMAL_POLICY",
                "harbor-cad-thermal-cpu-v1",
                "--setenv",
                "HARBOR_CAD_HOST_NETNS",
                os.readlink("/proc/self/ns/net"),
                native["thermal"],
                "run",
                "/inputs/request.json",
            ]
        )
        process = subprocess.run(command, capture_output=True, timeout=200, check=False)
        (root / f"launch-{name}.log").write_bytes(process.stdout + process.stderr)
        if process.returncode:
            raise RuntimeError(
                f"native prescribed snow thermal gate failed; originals retained at {work}"
            )
        receipt = json.loads((work / "thermal-receipt.json").read_text())
        mesh = json.loads((work / "mesh.json").read_text())
        nodes = {int(key): value for key, value in mesh["nodes"].items()}
        cells = {int(key): value for key, value in mesh["elements"].items()}
        checks, _, retained = bridge.verify(
            spec, nodes, cells, fem.read_dat((work / "reference.dat").read_text())
        )
        if (
            receipt["request_sha256"] != checksum(request)
            or receipt["native_field_sha256"] != checksum(work / "reference.dat")
            or receipt["mesh_sha256"] != checksum(work / "mesh.json")
            or checks != receipt["numerical_verification"]
            or receipt["sandbox"]["policy"] != "harbor-cad-thermal-cpu-v1"
            or len(receipt["sandbox"]["checks"]) != 8
            or not all(receipt["sandbox"]["checks"].values())
        ):
            raise ValueError(
                "complete independent original-field thermal and closure-only sandbox evidence required"
            )
        fields = json.loads((work / "thermal-fields.json").read_text())
        if fields["times"] != retained:
            # Native JSON entity keys are strings after decoding.
            normalized = json.loads(json.dumps(retained))
            if fields["times"] != normalized:
                raise ValueError(
                    "lossless retained original native thermal observations required"
                )
        result = {
            "case": name,
            "request": spec,
            "plan": planned,
            "receipt": receipt,
            "independent_checks": checks,
            "original_files_sha256": {
                p.name: checksum(p) for p in work.iterdir() if p.is_file()
            },
            "argv": command,
            "exit_code": process.returncode,
        }
        results.append(result)
        return result, fields, mesh

    spatial = []
    for resolution in (2, 4, 8):
        spec = copy.deepcopy(base)
        spec["thermal"]["resolution"] = resolution
        planned = prepare(spec)
        result, _, _ = run(
            f"snow-n{resolution}-dt40", planned["plan"]["thermal"], planned
        )
        spatial.append(
            result["independent_checks"]["temperature"]["normalized_max_abs_error"]
        )
    if not all(a > b for a, b in pairwise(spatial)):
        raise ValueError(
            f"equal-time snow plane-wall continuum errors must decrease under spatial refinement: {spatial}"
        )
    fixed = []
    for step in (80.0, 40.0, 20.0):
        spec = copy.deepcopy(base)
        spec["thermal"].update(resolution=8, max_step_s=step)
        planned = prepare(spec)
        name = f"snow-n8-dt{step:g}"
        if step == 40:
            result = next(r for r in results if r["case"] == name)
            fields = json.loads((root / name / "thermal-fields.json").read_text())
            mesh = json.loads((root / name / "mesh.json").read_text())
        else:
            result, fields, mesh = run(name, planned["plan"]["thermal"], planned)
        fixed.append((result, fields, mesh))
    temporal = temporal_self_convergence(
        [entry[0]["request"] for entry in fixed],
        [entry[1] for entry in fixed],
        [entry[2] for entry in fixed],
    )
    bare = copy.deepcopy(base["thermal"])
    bare.update(resolution=8, max_step_s=20.0)
    _, bare_fields, _ = run("bare-n8-dt20", bare, None)
    snow_fields = fixed[-1][1]
    temperature_differences = [
        min(
            snow["temperature_k"][node] - clean["temperature_k"][node]
            for node in snow["temperature_k"]
        )
        for snow, clean in zip(snow_fields["times"], bare_fields["times"], strict=True)
    ]
    if any(value <= 0 for value in temperature_differences):
        raise ValueError(
            "prescribed subzero insulation must retain more device heat than the same bare-wall reference"
        )
    for field, value in (
        ("coverage", "partial"),
        ("model", "deposition"),
        ("opening_model", "blocked_from_image"),
        ("maximum_omitted_capacity_ratio", 0.03),
        ("maximum_diffusion_timescale_ratio", 0.03),
        ("density", {"value": 10000.0, "unit": "kg/m3"}),
        ("thickness", {"value": 0.0, "unit": "mm"}),
        ("provenance", ""),
    ):
        changed = copy.deepcopy(base)
        changed["snow"][field] = value
        process = subprocess.run(
            [str(binary), "case", "validate-snow-reference", "/dev/stdin"],
            input=json.dumps(changed).encode(),
            capture_output=True,
            timeout=30,
            check=False,
        )
        if (
            not process.returncode
            or json.loads(process.stdout)["error"]["code"] != "invalid_input"
        ):
            raise ValueError(
                "unsupported snow prescription must reject before launching a native solver"
            )
        rejections.append(
            {
                "field": field,
                "exit_code": process.returncode,
                "reply": json.loads(process.stdout),
            }
        )
    report = {
        "schema_version": 1,
        "planner": str(binary),
        "planner_sha256": checksum(binary),
        "planner_qualification": "development source-built planner; package unqualified"
        if args.development_planner
        else "exact immutable package",
        "runtime": str(runtime),
        "runtime_sha256": checksum(runtime),
        "runtime_descriptor_kind": "explicit operation-only thermal worker"
        if args.worker_runtime
        else "standalone thermal reference",
        "thermal_closure": str(closure),
        "thermal_closure_sha256": checksum(closure),
        "adapter": native["thermal"],
        "source_verifier_sha256": checksum(repo / "adapters/thermal_history.py"),
        "results": results,
        "rejections": rejections,
        "spatial_errors_n2_n4_n8": spatial,
        "temporal_self_convergence": temporal,
        "snow_minus_bare_minimum_temperature_k": temperature_differences,
        "service_resources_before": before,
        "service_resources_after": service_resources(),
        "physical_validation": "unqualified",
        "scope": "native transient device temperature/energy under prescribed dry full-face quasi-steady snow resistance; explicit omitted-storage screens; no snow deposition/melting, blocked-opening flow, worker execution or physical qualification",
    }
    (root / "verification.json").write_text(
        json.dumps(report, allow_nan=False, indent=2)
    )
    print(
        json.dumps(
            {
                "report_sha256": checksum(root / "verification.json"),
                "results": len(results),
                "rejections": len(rejections),
                "spatial_errors": spatial,
                "temporal_self_convergence": temporal,
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
