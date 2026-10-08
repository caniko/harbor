"""Opt-in exact-package patched FCStd import to BREP/OCC C3D8 correspondence gate."""

import argparse
import hashlib
import itertools
import json
import math
import os
import shutil
import subprocess
import time
from pathlib import Path

from verify_native_cpu import verify_manifest
from verify_openlb_hip import service_resources
from verify_systemd import wait_admission_release, wait_job, wait_retention_release


def checksum(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify_box_mesh(spec, mesh):
    """Independent Cartesian-grid, oriented-cell and semantic-boundary checks."""
    n, tolerance, bounds = (
        spec["resolution"],
        spec["geometry_tolerance_m"],
        spec["bounds_m"],
    )
    nodes = {int(k): v for k, v in mesh["nodes"].items()}
    cells = mesh["elements"]
    if (
        mesh["coordinate_unit"] != "m"
        or mesh["element_type"] != "C3D8"
        or mesh["synthetic"] != spec["synthetic"]
        or mesh["positive_gauss_jacobians"] is not True
    ):
        raise ValueError(
            "unchanged mesh units, provenance and positive Jacobian evidence required"
        )
    if len(nodes) != (n + 1) ** 3 or len(cells) != n**3:
        raise ValueError("complete spatial refinement coverage required")
    widths = [bounds[2 * i + 1] - bounds[2 * i] for i in range(3)]
    positions = set(itertools.product(range(n + 1), repeat=3))
    observed = set()
    for xyz in nodes.values():
        ijk = tuple(round((xyz[i] - bounds[2 * i]) * n / widths[i]) for i in range(3))
        if (
            ijk not in positions
            or ijk in observed
            or any(
                abs(xyz[i] - (bounds[2 * i] + ijk[i] * widths[i] / n)) > tolerance
                for i in range(3)
            )
        ):
            raise ValueError(
                "native grid does not correspond to approved world-space CAD"
            )
        observed.add(ijk)
    if observed != positions:
        raise ValueError("missing native spatial grid locations")
    origins, volume = set(), 0.0
    for ids in cells.values():
        if len(ids) != 8 or len(set(ids)) != 8 or any(i not in nodes for i in ids):
            raise ValueError("complete unique native hexahedral connectivity required")
        xyz = [nodes[i] for i in ids]
        lower = [min(p[i] for p in xyz) for i in range(3)]
        upper = [max(p[i] for p in xyz) for i in range(3)]
        origin = tuple(
            round((lower[i] - bounds[2 * i]) * n / widths[i]) for i in range(3)
        )
        if origin in origins or any(not 0 <= p < n for p in origin):
            raise ValueError("overlapping or out-of-domain native cells")
        origins.add(origin)
        if any(abs(upper[i] - lower[i] - widths[i] / n) > tolerance for i in range(3)):
            raise ValueError("native cell extent differs from imported CAD refinement")
        corners = {
            tuple(round((p[i] - lower[i]) * n / widths[i]) for i in range(3))
            for p in xyz
        }
        if corners != set(itertools.product((0, 1), repeat=3)):
            raise ValueError("native cell is not a complete Cartesian hexahedron")
        a, b, c = ([xyz[j][i] - xyz[0][i] for i in range(3)] for j in (1, 3, 4))
        signed = (
            a[0] * (b[1] * c[2] - b[2] * c[1])
            - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0])
        )
        if signed <= 0.0 or not math.isclose(
            signed, math.prod(widths) / n**3, rel_tol=1e-10
        ):
            raise ValueError("independent native oriented-cell volume check failed")
        volume += signed
    if not math.isclose(
        volume, spec["volume_m3"], rel_tol=spec["volume_relative_tolerance"]
    ) or not math.isclose(
        mesh["integrated_volume_m3"], volume, rel_tol=spec["volume_relative_tolerance"]
    ):
        raise ValueError("independent CAD/OCC/mesh volume-conservation check failed")
    sets = mesh["boundary_node_sets"]
    if set(sets) != {axis + side for axis in "xyz" for side in ("min", "max")}:
        raise ValueError("six complete semantic face sets required")
    for name, ids in sets.items():
        axis = "xyz".index(name[0])
        edge = bounds[2 * axis + int(name.endswith("max"))]
        expected = {i for i, p in nodes.items() if abs(p[axis] - edge) <= tolerance}
        if (
            len(ids) != (n + 1) ** 2
            or set(ids) != expected
            or len(ids) != len(set(ids))
        ):
            raise ValueError("native semantic boundary is not the approved CAD plane")
    return {
        "coordinate_unit": "m",
        "nodes": len(nodes),
        "elements": len(cells),
        "oriented_volume_m3": volume,
        "relative_volume_error": abs(volume - spec["volume_m3"]) / spec["volume_m3"],
        "complete_cartesian_world_grid": True,
        "semantic_faces": "six complete approved world planes",
        "passed": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for option in (
        "executable",
        "runtime",
        "fixture-runtime",
        "mesh-runtime",
        "authority",
        "output",
    ):
        parser.add_argument(f"--{option}", type=Path, required=True)
    args = parser.parse_args()
    binary, runtime, fixture_runtime, mesh_runtime = [
        p.resolve(strict=True)
        for p in (
            args.executable,
            args.runtime,
            args.fixture_runtime,
            args.mesh_runtime,
        )
    ]
    if any(
        not p.is_relative_to("/nix/store") or not p.is_file()
        for p in (binary, runtime, fixture_runtime, mesh_runtime)
    ):
        raise ValueError("exact immutable packaged CAD/OCC/CLI stack required")
    native, fixture_native, mesh_native = [
        json.loads(p.read_text()) for p in (runtime, fixture_runtime, mesh_runtime)
    ]
    if not native.get("cad") or any(
        native.get(key) is not None
        for key in ("openlb", "render", "video", "filter", "fem", "thermal")
    ):
        raise ValueError("independent CAD-only worker runtime required")
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    before = service_resources()
    fixtures = root / "fixtures"
    fixtures.mkdir(mode=0o700)
    environment = {
        k: os.environ[k]
        for k in ("HOME", "XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS", "PATH")
        if k in os.environ
    }

    def standalone(bwrap, executable, output, inputs=None, argv=()):
        command = [
            bwrap,
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
        ]
        if inputs is not None:
            command += ["--ro-bind", str(inputs), "/inputs"]
        command += [executable, *argv]
        result = subprocess.run(
            command, env=environment, capture_output=True, timeout=120, check=False
        )
        (output / "process.log").write_bytes(result.stdout + result.stderr)
        return result, command

    process, fixture_command = standalone(
        fixture_native["bwrap"], fixture_native["fixture"], fixtures
    )
    if process.returncode or not (fixtures / "fixture-manifest.json").is_file():
        raise RuntimeError("controlled immutable FreeCAD fixture generation failed")
    fixture_manifest = json.loads((fixtures / "fixture-manifest.json").read_text())
    state = root / "state"
    endpoint = state / "worker.sock"
    profile = root / "profile.json"
    profile.write_text(
        json.dumps(
            {
                "schema_version": 1,
                "policy": "research",
                "allowed_input_root": str(root),
                "max_ram_bytes": 2 * 1024**3,
                "max_disk_bytes": 1024**3,
                "threads": 1,
                "timeout_seconds": 180,
                "native_runtime": str(runtime),
                "service_mode": "systemd",
            }
        )
    )

    def command(*argv):
        result = subprocess.run(
            [str(binary), *map(str, argv)],
            env=environment,
            capture_output=True,
            timeout=30,
            check=False,
        )
        reply = json.loads(result.stdout)
        if result.returncode:
            raise RuntimeError(reply)
        return reply

    worker, owned = None, []
    results, rejections = [], []
    try:
        with (root / "worker.log").open("xb") as log:
            worker = subprocess.Popen(
                [
                    str(binary),
                    "worker",
                    "--state",
                    str(state),
                    "--profile",
                    str(profile),
                    "--authority",
                    str(args.authority.resolve(strict=True)),
                ],
                env=environment,
                stdout=log,
                stderr=subprocess.STDOUT,
            )
            deadline = time.monotonic() + 15
            while not endpoint.exists():
                if worker.poll() is not None or time.monotonic() >= deadline:
                    raise RuntimeError("CAD worker startup failed; inspect worker.log")
                time.sleep(0.01)
            for fixture in fixture_manifest["fixtures"]:
                label = fixture["label"]
                source = fixtures / fixture["source"]
                original = source.read_bytes()
                if checksum(source) != fixture["sha256"]:
                    raise ValueError("controlled source fixture changed")
                source.chmod(0o400)
                case = command("case", "init")
                case["geometry"] = {
                    "source": str(source.relative_to(root)),
                    "sha256": fixture["sha256"],
                    "synthetic": True,
                }
                case["regions"] = ["solid"]
                case_path = root / f"case-{label}.json"
                case_path.write_text(json.dumps(case))
                approval = command(
                    "case",
                    "plan-cad-inspection",
                    case_path,
                    "--policy",
                    "research",
                    "--max-artifact-bytes",
                    str(64 * 1024**2),
                )
                plan_path = root / f"plan-{label}.json"
                plan_path.write_text(json.dumps(approval["plan"]))
                job = command(
                    "--socket",
                    endpoint,
                    "job",
                    "submit",
                    plan_path,
                    "--approve",
                    approval["approval_digest"],
                    "--idempotency-key",
                    label,
                )["data"]
                owned.append(job["unit"])
                outcome = wait_job(
                    str(binary), endpoint, job["id"], {"succeeded"}, timeout=180
                )
                wait_admission_release(state, job)
                wait_retention_release(state, job)
                report = command("--socket", endpoint, "cad", "regions", job["id"])[
                    "data"
                ]
                if (
                    report["physical_validation"] != "unqualified"
                    or report["coordinate_unit"] != "m"
                    or report["placement_translation_unit"] != "mm"
                ):
                    raise ValueError("historical CAD units/provenance drift")
                bundle = root / f"bundle-{label}"
                command("artifact", "export", "--state", state, job["id"], bundle)
                records = verify_manifest(bundle)
                manifest = json.loads((bundle / "brep-manifest.json").read_text())
                if (
                    manifest["synthetic"] is not True
                    or manifest["gap_healing"] is not False
                    or len(manifest["regions"]) != 1
                ):
                    raise ValueError("unambiguous original named BREP region required")
                region = manifest["regions"][0]
                if (
                    region["region_name"] != "solid"
                    or region["path"] != "solid.brep"
                    or region["source_transform"] != fixture["source_transform"]
                    or region["bounds_m"] != fixture["bounds_m"]
                    or region["volume_m3"] != fixture["volume_m3"]
                ):
                    raise ValueError(
                        "importer changed controlled placement, SI bounds or original solid volume"
                    )
                brep = bundle / "solid.brep"
                if (
                    region["sha256"] != checksum(brep)
                    or region["bytes"] != brep.stat().st_size
                    or source.read_bytes() != original
                ):
                    raise ValueError(
                        "original or exported imported geometry bytes changed"
                    )
                isolation = json.loads((bundle / "import-isolation.json").read_text())
                if (
                    any(
                        isolation[key] is not True
                        for key in (
                            "package_mounts_read_only",
                            "plan_read_only",
                            "input_read_only",
                            "gpu_devices_absent",
                            "host_session_environment_absent",
                            "no_new_privileges",
                        )
                    )
                    or isolation["net_namespace"] == isolation["host_net_namespace"]
                    or isolation["network_interfaces"] != ["lo"]
                    or int(isolation["effective_capabilities"], 16) != 0
                ):
                    raise ValueError("patched CAD import isolation failed")
                base = {
                    "schema_version": 1,
                    "synthetic": True,
                    "backend": "cpu",
                    "formulation": "imported_axis_aligned_box",
                    "geometry_provenance": "controlled synthetic patched FreeCAD import",
                    "brep_file": "solid.brep",
                    "brep_sha256": region["sha256"],
                    "brep_bytes": region["bytes"],
                    "region_name": "solid",
                    "bounds_m": region["bounds_m"],
                    "volume_m3": region["volume_m3"],
                    "source_unit": region["source_unit"],
                    "scale_to_m": region["scale_to_m"],
                    "placement_translation_unit": region["placement_translation_unit"],
                    "source_transform": region["source_transform"],
                    "geometry_tolerance_m": 1e-6,
                    "volume_relative_tolerance": 1e-10,
                }

                def invoke(name, spec, source_brep=brep):
                    inputs = root / f"mesh-input-{name}"
                    inputs.mkdir(mode=0o700)
                    shutil.copyfile(source_brep, inputs / "solid.brep")
                    request = inputs / "request.json"
                    request.write_text(json.dumps(spec, allow_nan=False, indent=2))
                    output = root / f"mesh-{name}"
                    output.mkdir(mode=0o700)
                    status, argv = standalone(
                        mesh_native["bwrap"],
                        mesh_native["cad_mesh"],
                        output,
                        inputs,
                        ("mesh", "/inputs/request.json"),
                    )
                    return status, argv, output, request

                for resolution in (2, 4, 8):
                    spec = {**base, "resolution": resolution}
                    status, argv, output, request = invoke(
                        f"{label}-n{resolution}", spec
                    )
                    if status.returncode:
                        raise RuntimeError(
                            f"imported native CAD mesh failed; inspect {output}"
                        )
                    receipt = json.loads((output / "cad-mesh-receipt.json").read_text())
                    mesh = json.loads((output / "mesh.json").read_text())
                    checks = verify_box_mesh(spec, mesh)
                    if (
                        receipt["mesh_sha256"] != checksum(output / "mesh.json")
                        or receipt["request_sha256"] != checksum(request)
                        or receipt["brep_sha256"] != region["sha256"]
                    ):
                        raise ValueError(
                            "native CAD source/request/mesh hashes changed"
                        )
                    if (
                        receipt["source_transform"] != fixture["source_transform"]
                        or receipt["world_bounds_m"] != fixture["bounds_m"]
                        or receipt["physical_validation"] != "unqualified"
                        or receipt["gap_healing"]
                    ):
                        raise ValueError("native placement or mesh applicability drift")
                    results.append(
                        {
                            "fixture": label,
                            "resolution": resolution,
                            "job": outcome,
                            "cad_report": report,
                            "records": len(records),
                            "request": spec,
                            "argv": argv,
                            "receipt": receipt,
                            "independent_checks": checks,
                        }
                    )
                if label == "origin":
                    cases = [
                        (
                            "source-hash",
                            {**base, "resolution": 4, "brep_sha256": "0" * 64},
                            brep,
                        ),
                        ("unit", {**base, "resolution": 4, "source_unit": "m"}, brep),
                        (
                            "weak-gate",
                            {
                                **base,
                                "resolution": 4,
                                "volume_relative_tolerance": 0.001,
                            },
                            brep,
                        ),
                        (
                            "world-bounds",
                            {
                                **base,
                                "resolution": 4,
                                "bounds_m": [0.1, 0.12, 0.0, 0.01, 0.0, 0.01],
                            },
                            brep,
                        ),
                        (
                            "path",
                            {**base, "resolution": 4, "brep_file": "../solid.brep"},
                            brep,
                        ),
                    ]
                    for negative, entry in fixture_manifest["negative_breps"].items():
                        cases.append(
                            (
                                negative,
                                {
                                    **base,
                                    "resolution": 4,
                                    "brep_sha256": entry["sha256"],
                                    "brep_bytes": entry["bytes"],
                                },
                                fixtures / entry["path"],
                            )
                        )
                    for name, spec, source_brep in cases:
                        status, argv, output, _ = invoke(
                            f"reject-{name}", spec, source_brep
                        )
                        if (
                            not status.returncode
                            or (output / "mesh.json").exists()
                            or (output / "cad-mesh-receipt.json").exists()
                        ):
                            raise ValueError(
                                f"unsupported imported geometry accepted: {name}"
                            )
                        rejections.append(
                            {
                                "case": name,
                                "exit_code": status.returncode,
                                "argv": argv,
                                "log_sha256": checksum(output / "process.log"),
                            }
                        )
        report = {
            "schema_version": 1,
            "binary": str(binary),
            "runtime": str(runtime),
            "fixture_runtime": str(fixture_runtime),
            "mesh_runtime": str(mesh_runtime),
            "runtime_sha256": checksum(runtime),
            "fixture_runtime_sha256": checksum(fixture_runtime),
            "mesh_runtime_sha256": checksum(mesh_runtime),
            "fixture_command": fixture_command,
            "results": results,
            "rejections": rejections,
            "service_resources_before": before,
            "service_resources_after": service_resources(),
            "physical_validation": "unqualified",
            "scope": "controlled security-patched FCStd import to preserved origin/translated BREP and independent imported box C3D8 mesh; no solver/contact qualification",
        }
        (root / "verification.json").write_text(
            json.dumps(report, allow_nan=False, indent=2)
        )
        print(json.dumps(report, allow_nan=False, indent=2))
    finally:
        for unit in owned:
            subprocess.run(
                ["systemctl", "--user", "stop", unit], check=False, capture_output=True
            )
        if worker is not None and worker.poll() is None:
            worker.terminate()
            worker.wait(timeout=5)


if __name__ == "__main__":
    main()
