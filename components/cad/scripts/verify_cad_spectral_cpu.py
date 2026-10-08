"""Original-facet direct spectral references; native execution is separately scoped."""

import argparse
import hashlib
import importlib.util
import json
import math
import os
import shutil
import struct
import subprocess
from pathlib import Path

from verify_openlb_hip import service_resources
from verify_spectral_cpu import checksum


def module(path):
    spec = importlib.util.spec_from_file_location("cad_spectral_transport", path)
    bridge = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(bridge)
    return bridge


def original_box(size, origin):
    """Manufactured mm facets; original winding/order, never imported CAD evidence."""
    vertices = [
        (origin[0] + x, origin[1] + y, origin[2] + z)
        for x in (0, size[0])
        for y in (0, size[1])
        for z in (0, size[2])
    ]
    faces = [
        (0, 1, 3),
        (0, 3, 2),
        (4, 6, 7),
        (4, 7, 5),
        (0, 4, 5),
        (0, 5, 1),
        (2, 3, 7),
        (2, 7, 6),
        (0, 2, 6),
        (0, 6, 4),
        (1, 5, 7),
        (1, 7, 3),
    ]
    data = bytearray(80) + struct.pack("<I", 12)
    for face in faces:
        points = [vertices[i] for i in face]
        a, b = [[points[j][i] - points[0][i] for i in range(3)] for j in (1, 2)]
        vector = [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
        length = math.sqrt(math.fsum(v * v for v in vector))
        data.extend(
            struct.pack(
                "<12fH",
                *[v / length for v in vector],
                *[v for p in points for v in p],
                0,
            )
        )
    return bytes(data)


def fixture(inputs, *, direction=(0, 0, -1), translated=False, blocker=None):
    request = json.loads(
        (
            Path(__file__).resolve().parents[1] / "examples/cad-spectral-scene.json"
        ).read_text()
    )
    request["geometry_tolerance"] = {"value": 1e-8, "unit": "m"}
    request["materials"][0]["ageing_action"] = {
        "availability": "known",
        "value": [0.2, 0.8],
        "provenance": "manufactured distinct action curve; no lifetime calibration",
        "synthetic": True,
    }
    origin = [10, 20, 30] if translated else [0, 0, 0]
    regions = []
    boxes = [("solid", [1, 2, 3], origin)]
    if blocker:
        boxes.append(("blocker", [1 if blocker == "full" else 0.5, 2, 3], [0, 0, 6]))
        request["assignments"].append(
            {
                "region_name": "blocker",
                "material_name": "synthetic_opaque",
                "provenance": "explicit manufactured disjoint opaque shadow box",
            }
        )
    for name, size, position in boxes:
        data = original_box(size, position)
        (inputs / (name + ".stl")).write_bytes(data)
        regions.append(
            {
                "assignment": next(
                    v for v in request["assignments"] if v["region_name"] == name
                ),
                "source": {
                    "geometry": {
                        "bounds_m": [
                            v * 0.001
                            for axis in range(3)
                            for v in (position[axis], position[axis] + size[axis])
                        ],
                        "synthetic": True,
                        "geometry_tolerance_m": 1e-8,
                    }
                },
                "original_triangles": {
                    "path": name + ".stl",
                    "sha256": hashlib.sha256(data).hexdigest(),
                    "bytes": len(data),
                },
                "geometry": {"triangles": 12},
            }
        )
    return {
        "schema_version": 1,
        "synthetic": True,
        "backend": "cpu",
        "variant": "scalar_spectral",
        "precision": "Float32",
        "formulation": "opaque_lambertian_direct_only",
        "scene": {
            "schema_version": 1,
            "scene_id": "a" * 64,
            "request": request,
            "regions": regions,
            "missing_inputs": [],
            "transport_readiness": "prepared_not_executed",
            "ageing_readiness": "prepared_not_executed",
            "executed": False,
            "physical_validation": "unqualified",
            "limitations": [
                "Manufactured original STL, not executed importer or registered-source approval"
            ],
        },
        "source": {
            "kind": "directional",
            "propagation_direction": list(direction),
            "irradiance": [
                {"value": 1.0, "unit": "W/(m2*nm)"},
                {"value": 2e9, "unit": "W/(m2*m)"},
            ],
        },
        "source_provenance": "explicit manufactured collimated source; no atmosphere inference",
        "history": [
            {"time": {"value": 0, "unit": "s"}, "scale": 1.0},
            {"time": {"value": 1, "unit": "h"}, "scale": 3.0},
        ],
        "history_interpolation": "piecewise_linear_prescribed_scale",
        "history_provenance": "explicit manufactured source-amplitude history",
        "samples_per_triangle": 2048 if blocker == "partial" else 64,
        "seeds": [17, 29, 43],
        "relative_tolerance": 0.02,
        "maximum_geometry_rounding_error_m": 1e-9,
    }


def verify(work, spec, normalized, bridge, request):
    receipt = json.loads((work / "cad-spectral-receipt.json").read_text())
    checks = {
        "operation_closure_only",
        "no_gpu_nodes",
        "no_sysfs",
        "no_host_home",
        "no_session_bus",
        "no_worker_socket",
        "network_namespace_isolated",
        "descriptor_readonly",
        "original_source_readonly",
    }
    if (
        receipt["input"] != spec
        or receipt["request_sha256"] != checksum(request)
        or receipt["sandbox"]["policy"] != bridge.POLICY
        or set(receipt["sandbox"]["checks"]) != checks
        or any(v is not True for v in receipt["sandbox"]["checks"].values())
        or receipt["versions"] != {"mitsuba": "3.9.1", "drjit": "1.5.0"}
        or receipt["backend"] != "cpu"
        or receipt["variant"] != "scalar_spectral"
        or receipt["precision"] != "Float32"
        or receipt["executed"] is not True
        or receipt["software_fallback"] is not False
        or receipt["physical_validation"] != "unqualified"
        or receipt["formulation"] != "opaque_lambertian_direct_only"
        or [row["seed"] for row in receipt["observations"]] != spec["seeds"]
        or receipt["sampling_convergence"] != "not_assessed"
        or receipt["interreflection"] != "excluded_by_explicit_direct_only_model"
    ):
        raise ValueError(
            "complete native input, scope, version, isolation and seed attestations required"
        )
    for observation in receipt["observations"]:
        record = observation["original"]
        path = work / record["path"]
        if (
            record["path"] != f"triangles-{observation['seed']}.csv"
            or path.is_symlink()
            or not path.is_file()
            or path.stat().st_size != record["bytes"]
            or path.stat().st_size > 256 * 1024**2
            or checksum(path) != record["sha256"]
        ):
            raise ValueError(
                "complete unchanged native original packet identity required"
            )
        if (
            bridge.reconstruct(spec, normalized, receipt["geometry_conversions"], path)
            != observation["facets"]
        ):
            raise ValueError(
                "independent complete original packet reconstruction differs"
            )
        for facet in observation["facets"]:
            if not math.isclose(
                facet["power_w"]["absorbed"] + facet["power_w"]["reflected_outgoing"],
                facet["power_w"]["incident"],
                rel_tol=1e-12,
                abs_tol=0.0,
            ):
                raise ValueError(
                    "unchanged opaque original material energy closure required"
                )
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path)
    parser.add_argument("--dependency-runtime", type=Path)
    parser.add_argument("--development-bridge", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    candidate = args.development_bridge
    if bool(args.runtime) == bool(candidate) or bool(args.dependency_runtime) != bool(
        candidate
    ):
        parser.error(
            "either exact --runtime or explicit --development-bridge and --dependency-runtime required"
        )
    descriptor = (args.runtime or args.dependency_runtime).resolve(strict=True)
    if not descriptor.is_relative_to("/nix/store") or not descriptor.is_file():
        raise ValueError("immutable native dependency descriptor required")
    runtime = json.loads(descriptor.read_text())
    closure = Path(runtime["spectral_closure"]).resolve(strict=True)
    paths = closure.read_text().splitlines()
    if (
        not paths
        or len(paths) > 4096
        or len(paths) != len(set(paths))
        or any(
            Path(p).parent != Path("/nix/store") or not Path(p).exists() for p in paths
        )
    ):
        raise ValueError("bounded immutable operation-only native closure required")
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    (root / ".doty-protect").write_text(
        "Preserve native original-facet optical references, source candidates and failed attempts.\n"
    )
    if candidate:
        candidate = candidate.resolve(strict=True)
        overlay = root / "candidate"
        overlay.mkdir()
        for name in (
            "cad_spectral_transport.py",
            "spectral_reference.py",
            "fem_reference.py",
        ):
            shutil.copyfile(candidate.with_name(name), overlay / name)
        launcher = Path(runtime["atmospheric_spectral"])
        interpreter = launcher.read_text().split("exec ", 1)[1].split()[0]
        if not interpreter.startswith("/nix/store/") or not Path(interpreter).is_file():
            raise ValueError(
                "exact isolated immutable native dependency interpreter required"
            )
        executable = [interpreter, "-B", "/candidate/cad_spectral_transport.py"]
        qualification = "unqualified explicitly retained source bridge overlay; exact immutable native dependencies"
    else:
        if (
            set(runtime)
            != {
                "schema_version",
                "bwrap",
                "cad_spectral",
                "spectral_closure",
                "backend",
                "precision",
                "policy",
                "qualification",
            }
            or runtime["policy"] != "harbor-cad-cad-spectral-direct-cpu-v1"
            or runtime["backend"] != "cpu"
            or runtime["precision"] != "Float32"
        ):
            raise ValueError("exact native CPU original-facet runtime required")
        executable = [runtime["cad_spectral"]]
        qualification = (
            "exact immutable direct-only adapter and native operation sandbox"
        )
    bridge = module(repo / "adapters/cad_spectral_transport.py")
    before = service_resources()
    results = []
    for name, options in (
        ("normal-z", {}),
        ("normal-x", {"direction": (-1, 0, 0)}),
        ("normal-y", {"direction": (0, -1, 0)}),
        ("oblique", {"direction": (-0.6, 0, -0.8)}),
        ("translated", {"translated": True}),
        ("full-shadow", {"blocker": "full"}),
        ("partial-shadow", {"blocker": "partial"}),
        ("black", {}),
        ("white", {}),
        ("missing-ageing", {}),
    ):
        inputs = root / ("input-" + name)
        inputs.mkdir()
        spec = fixture(inputs, **options)
        material = spec["scene"]["request"]["materials"][0]
        if name in ("black", "white"):
            reflectance = 0.0 if name == "black" else 1.0
            material["response"]["value"].update(
                reflectance=[reflectance] * 2, absorptivity=[1.0 - reflectance] * 2
            )
        if name == "missing-ageing":
            material["ageing_action"] = {
                "availability": "missing",
                "reason": "no calibrated ageing curve supplied",
            }
            spec["scene"]["missing_inputs"] = ["synthetic_opaque.ageing_action"]
            spec["scene"]["ageing_readiness"] = "missing_inputs"
        normalized = bridge.normalize(spec, inputs)
        request = inputs / "request.json"
        request.write_text(json.dumps(spec, allow_nan=False))
        identities = {path.name: checksum(path) for path in inputs.iterdir()}
        work = root / name
        work.mkdir()
        command = [
            runtime["bwrap"],
            "--die-with-parent",
            "--new-session",
            "--unshare-all",
            "--dir",
            "/nix/store",
        ]
        for path in paths:
            command += ["--ro-bind", path, path]
        if candidate:
            command += ["--ro-bind", str(overlay), "/candidate"]
        command += [
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
            "--ro-bind",
            str(inputs),
            "/source",
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
            "HARBOR_CAD_CAD_SPECTRAL_POLICY",
            bridge.POLICY,
            "--setenv",
            "HARBOR_CAD_HOST_NETNS",
            os.readlink("/proc/self/ns/net"),
            *executable,
            "reference",
            "/inputs/request.json",
        ]
        (root / (name + "-command.json")).write_text(json.dumps(command, indent=2))
        with (root / (name + "-process.log")).open("xb") as log:
            process = subprocess.run(
                command, stdout=log, stderr=subprocess.STDOUT, timeout=600, check=False
            )
        (root / (name + "-exit.txt")).write_text(str(process.returncode) + "\n")
        if process.returncode:
            raise RuntimeError((root / (name + "-process.log")).read_text()[-12000:])
        receipt = verify(work, spec, normalized, bridge, request)
        # Projected-area oracle is independent of both native point sampling and
        # its reconstruction. Partial/full blockers leave the complete projected
        # union unchanged, including the blocker's separately observed top.
        direction = spec["source"]["propagation_direction"]
        projected = math.fsum(
            abs(v) * area for v, area in zip(direction, [6e-6, 3e-6, 2e-6])
        )
        errors = [
            abs(
                math.fsum(f["power_w"]["incident"] for f in o["facets"])
                / (150.0 * projected)
                - 1.0
            )
            for o in receipt["observations"]
        ]
        if max(errors) > spec["relative_tolerance"]:
            raise ValueError(
                "unchanged complete-scene projected-area analytical power gate exceeded"
            )
        if {path.name: checksum(path) for path in inputs.iterdir()} != identities:
            raise ValueError("original approved triangle and optical bytes changed")
        results.append(
            {
                "case": name,
                "input": spec,
                "original_files_sha256": identities,
                "receipt_sha256": checksum(work / "cad-spectral-receipt.json"),
                "packet_sha256": [
                    o["original"]["sha256"] for o in receipt["observations"]
                ],
                "projected_area_reference_m2": projected,
                "power_relative_errors": errors,
            }
        )
    report = {
        "schema_version": 1,
        "package_qualification": qualification,
        "runtime": str(descriptor),
        "runtime_sha256": checksum(descriptor),
        "candidate_sha256": {p.name: checksum(p) for p in overlay.iterdir()}
        if candidate
        else None,
        "results": results,
        "service_resources_before": before,
        "service_resources_after": service_resources(),
        "physical_validation": "unqualified",
        "scope": "manufactured original box-facet direct irradiance, visibility, original-area power and prescribed dose; no importer or registered-source worker, interreflection, atmosphere, GPU or physical qualification",
    }
    (root / "verification.json").write_text(
        json.dumps(report, indent=2, allow_nan=False)
    )
    print(
        json.dumps(
            {
                "cases": len(results),
                "seed_observations": len(results) * 3,
                "report_sha256": checksum(root / "verification.json"),
                "package_qualification": qualification,
            }
        )
    )


if __name__ == "__main__":
    main()
