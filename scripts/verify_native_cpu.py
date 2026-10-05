"""Opt-in packaged FreeCAD → CPU OpenLB → verified portable bundle, CLI and MCP.

Requires the declared packages and an existing systemd user manager. This checks
the synthetic low-Mach fixture; it never qualifies CUDA or physical engineering.
"""

import argparse
import asyncio
import hashlib
import json
import math
import os
import shutil
import subprocess
import time
import xml.etree.ElementTree as ET
from pathlib import Path

from verify_openlb_cpu import read_vti
from verify_systemd import wait_job, wait_retention_release


def verify_manifest(root):
    manifests = json.loads((root / "manifest.json").read_text())
    for manifest in manifests:
        relative = Path(manifest["path"])
        if relative.is_absolute() or ".." in relative.parts:
            raise ValueError("portable bundle path escape")
        path = root / relative
        if path.is_symlink() or not path.is_file():
            raise ValueError("portable bundle must contain regular closed records")
        data = path.read_bytes()
        if (
            len(data) != manifest["bytes"]
            or hashlib.sha256(data).hexdigest() != manifest["sha256"]
        ):
            raise ValueError("portable bundle checksum/size mismatch")
    return manifests


def verify_bundle(root, resolution):
    manifests = verify_manifest(root)
    execution = json.loads((root / "execution.json").read_text())
    assert execution["job"]["state"] == "succeeded"
    assert execution["plan"]["case"]["resolution"] == resolution
    assert execution["physical_validation"] == "unqualified"
    owner = json.loads((root / "service-owner.json").read_text())
    assert owner["unit"] == execution["job"]["unit"]
    assert owner["invocation"] == execution["job"]["invocation_id"]
    assert owner["main_pid"] > 0 and owner["control_group"].endswith(owner["unit"])
    runtime = execution["host_profile"]["native_runtime"]
    assert (root / "native-runtime.json").read_bytes() == Path(runtime).read_bytes()
    normalized = json.loads((root / "native-plan.json").read_text())
    assert normalized["case"]["length"] == {"value": 0.02, "unit": "m"}
    cad = json.loads((root / "cad_fixture-receipt.json").read_text())
    assert cad["executed"] is True and cad["backend"] == "cpu"
    assert tuple(map(int, cad["version"][:3])) >= (1, 1, 4)
    assert (root / "source.FCStd").is_file() and (root / "fluid.stl").is_file()
    regions = json.loads((root / "regions.json").read_text())
    assert regions["synthetic"] is True and regions["gap_healing"] is False
    assert {r["name"] for r in regions["regions"]} == {"fluid"}
    assert regions["regions"][0]["triangles"] > 0
    receipt = json.loads((root / "openlb-receipt.json").read_text())
    assert receipt["backend"] == "cpu" and receipt["executed"] is True
    assert receipt["software_fallback"] is False and receipt["precision"] == "float64"
    assert receipt["physical_validation"] == "unqualified"
    assert receipt["lattice_mach"] <= 0.1
    assert receipt["fluid_cells"] == 2 * resolution**3
    assert [t["requested_s"] for t in receipt["retained_times"]] == [0, 10, 20]
    collection_path = root / "tmp/vtkData/channel.pvd"
    collection = ET.parse(collection_path).findall("Collection/DataSet")
    assert [int(item.attrib["timestep"]) for item in collection] == [
        t["step"] for t in receipt["retained_times"]
    ]
    last_fields = None
    for item in collection:
        multiblock = collection_path.parent / item.attrib["file"]
        for dataset in ET.parse(multiblock).findall(".//DataSet"):
            last_fields = read_vti(multiblock.parent / dataset.attrib["file"])
    image, extent, shape, fields = last_fields
    nx, ny, nz = shape
    origin = list(map(float, image.attrib["Origin"].split()))
    spacing = list(map(float, image.attrib["Spacing"].split()))
    components, velocity = fields["physVelocity"]
    assert components == 3 and "physPressure" in fields and "geometry" in fields
    samples = []
    for j in range(ny):
        y = origin[1] + (extent[2] + j) * spacing[1]
        if 0 < y < 0.01:
            point = (nz // 2 * ny + j) * nx + nx // 2
            samples.append((velocity[point * 3], 0.001 * y * (0.01 - y) / (2 * 1e-5)))
    error = math.sqrt(
        sum((a - b) ** 2 for a, b in samples) / sum(b**2 for _, b in samples)
    )
    assert math.isfinite(error) and error <= 0.05
    return {
        "resolution": resolution,
        "velocity_relative_l2": error,
        "cad": cad,
        "solver": receipt,
        "artifacts": len(manifests),
        "bundle_checksums": "verified",
        "physical_validation": "unqualified",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path, required=True)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--mcp", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    binary, runtime, mcp = (
        str(p.resolve(strict=True)) for p in (args.executable, args.runtime, args.mcp)
    )
    if any(
        not Path(p).is_relative_to("/nix/store") or not Path(p).is_file()
        for p in (binary, runtime, mcp)
    ):
        raise ValueError("exact packaged CLI, runtime and MCP required")
    root = args.output.resolve()
    root.mkdir(parents=True, mode=0o700, exist_ok=False)
    socket = root / "state/worker.sock"
    profile = root / "profile.json"
    profile.write_text(
        json.dumps(
            {
                "schema_version": 1,
                "policy": "research",
                "allowed_input_root": str(root),
                "max_ram_bytes": 2 * 1024**3,
                "max_disk_bytes": 256 * 1024**2,
                "threads": 1,
                "timeout_seconds": 180,
                "native_runtime": runtime,
                "service_mode": "systemd",
            }
        )
    )
    owned = []
    # Keep only the user-manager connection needed by the worker. Native stages
    # still clear their entire inherited environment at the sandbox boundary.
    environment = {
        k: os.environ[k]
        for k in ("HOME", "XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS", "PATH")
        if k in os.environ
    }

    def command(*values):
        return json.loads(
            subprocess.check_output(
                [binary, *map(str, values)], env=environment, timeout=30
            )
        )

    with (root / "worker.log").open("w") as log:
        worker = subprocess.Popen(
            [
                binary,
                "worker",
                "--state",
                str(root / "state"),
                "--profile",
                str(profile),
            ],
            env=environment,
            stdout=log,
            stderr=subprocess.STDOUT,
        )
        try:
            deadline = time.monotonic() + 10
            while not socket.exists():
                if worker.poll() is not None or time.monotonic() >= deadline:
                    raise RuntimeError(
                        "packaged worker startup failed; inspect worker.log"
                    )
                time.sleep(0.02)
            results = []
            for resolution in (8, 16):
                case = command("case", "init")
                case.update(
                    length={"value": 0.02, "unit": "m"},
                    acceleration={"value": 0.001, "unit": "m/s2"},
                    resolution=resolution,
                    max_time_s=20,
                )
                case["applicability"]["formulation"] = "periodic_forced_channel"
                case["applicability"]["numerical_tolerance"] = 0.05
                case_path = root / f"case-{resolution}.json"
                case_path.write_text(json.dumps(case))
                planned = command("case", "plan-openlb-reference", case_path)
                plan_path = root / f"plan-{resolution}.json"
                plan_path.write_text(json.dumps(planned["plan"]))
                if resolution == 8:
                    job = command(
                        "--socket",
                        socket,
                        "job",
                        "submit",
                        plan_path,
                        "--approve",
                        planned["approval_digest"],
                        "--idempotency-key",
                        "native-cli",
                    )["data"]
                else:

                    async def submit_mcp(
                        case,
                        planned,
                        tool="case_plan_openlb_reference",
                        key="native-mcp",
                    ):
                        from mcp import Client
                        from mcp.client.stdio import StdioServerParameters

                        async with Client(
                            StdioServerParameters(
                                command=mcp,
                                args=["--profile", "all"],
                                env={**environment, "HARBOR_CAD_SOCKET": str(socket)},
                            )
                        ) as client:
                            result = await client.call_tool(tool, {"case": case})
                            assert (
                                not result.is_error
                                and result.structured_content == planned["plan"]
                            )
                            result = await client.call_tool(
                                "job_submit",
                                {
                                    "plan": result.structured_content,
                                    "approved_digest": planned["approval_digest"],
                                    "idempotency_key": key,
                                },
                            )
                            assert (
                                not result.is_error and result.structured_content["id"]
                            )
                            return result.structured_content

                    job = asyncio.run(submit_mcp(case, planned))
                owned.append(job["unit"])
                wait_job(binary, socket, job["id"], {"succeeded"}, timeout=190)
                wait_retention_release(root / "state", job)
                bundle = root / f"bundle-{resolution}"
                command(
                    "artifact", "export", "--state", root / "state", job["id"], bundle
                )
                results.append(verify_bundle(bundle, resolution))
            assert (
                results[1]["velocity_relative_l2"] < results[0]["velocity_relative_l2"]
            )
            # Inspect a real generated FCStd through the same approval/snapshot
            # path, independently of solver execution. Preserve its synthetic
            # provenance and never promote it into external engineering evidence.
            source = root / "approved.FCStd"
            shutil.copyfile(root / "bundle-8/source.FCStd", source)
            source_bytes = source.read_bytes()
            source_digest = hashlib.sha256(source_bytes).hexdigest()
            source.chmod(0o400)
            case["geometry"] = {
                "source": source.name,
                "sha256": source_digest,
                "synthetic": True,
            }
            # The saved fixture contains a fluid solid; its solver walls are
            # boundary surfaces, not named solids in the imported document.
            case["regions"] = ["fluid"]
            inspection_case = root / "inspection-case.json"
            inspection_case.write_text(json.dumps(case))
            inspection = command("case", "plan-cad-inspection", inspection_case)
            inspection_path = root / "inspection-plan.json"
            inspection_path.write_text(json.dumps(inspection["plan"]))
            imports = []
            for interface in ("CLI", "MCP", "missing-region", "changed-source"):
                if interface == "changed-source":
                    source.chmod(0o600)
                    source.write_bytes(b"changed after immutable approval")
                if interface == "MCP":
                    job = asyncio.run(
                        submit_mcp(
                            case, inspection, "cad_plan_inspection", "inspect-mcp"
                        )
                    )
                elif interface == "missing-region":
                    missing = {**case, "regions": ["fluid", "wall"]}
                    missing_case = root / "missing-region-case.json"
                    missing_case.write_text(json.dumps(missing))
                    missing_plan = command("case", "plan-cad-inspection", missing_case)
                    missing_path = root / "missing-region-plan.json"
                    missing_path.write_text(json.dumps(missing_plan["plan"]))
                    job = command(
                        "--socket",
                        socket,
                        "job",
                        "submit",
                        missing_path,
                        "--approve",
                        missing_plan["approval_digest"],
                        "--idempotency-key",
                        "inspect-missing-region",
                    )["data"]
                else:
                    job = command(
                        "--socket",
                        socket,
                        "job",
                        "submit",
                        inspection_path,
                        "--approve",
                        inspection["approval_digest"],
                        "--idempotency-key",
                        f"inspect-{interface}",
                    )["data"]
                owned.append(job["unit"])
                terminal = (
                    "failed"
                    if interface in {"changed-source", "missing-region"}
                    else "succeeded"
                )
                outcome = wait_job(binary, socket, job["id"], {terminal}, timeout=190)
                wait_retention_release(root / "state", job)
                bundle = root / f"inspection-{interface}"
                command(
                    "artifact", "export", "--state", root / "state", job["id"], bundle
                )
                manifests = verify_manifest(bundle)
                if interface == "changed-source":
                    assert "source CAD checksum/size mismatch" in outcome["error"]
                    assert not (bundle / "input.FCStd").exists()
                    assert not (bundle / "cad_inspect-receipt.json").exists()
                    source.write_bytes(source_bytes)
                    source.chmod(0o400)
                elif interface == "missing-region":
                    assert (bundle / "input.FCStd").read_bytes() == source_bytes
                    assert not (bundle / "cad_inspect-receipt.json").exists()
                    error = json.loads(
                        (bundle / "failed-native/import-error.json").read_text()
                    )
                    assert "missing or ambiguous named regions" in error["error"]
                else:
                    assert (
                        source.read_bytes()
                        == (bundle / "input.FCStd").read_bytes()
                        == source_bytes
                    )
                    assert (
                        source.stat().st_ino != (bundle / "input.FCStd").stat().st_ino
                    )
                    assert not any(
                        m["path"] == "openlb-receipt.json" for m in manifests
                    )
                    receipt = json.loads(
                        (bundle / "cad_inspect-receipt.json").read_text()
                    )
                    assert (
                        receipt["executed"]
                        and receipt["backend"] == "cpu"
                        and not receipt["software_fallback"]
                    )
                    regions = json.loads((bundle / "regions.json").read_text())
                    assert regions["synthetic"] and {
                        r["name"] for r in regions["regions"]
                    } == {"fluid"}
                    assert math.isclose(
                        regions["regions"][0]["volume_m3"],
                        0.02 * 0.01 * 0.01,
                        rel_tol=1e-12,
                    )
                imports.append(
                    {
                        "interface": interface,
                        "job": outcome,
                        "records": len(manifests),
                        "source_sha256": source_digest,
                    }
                )
            report = {
                "fixture": "synthetic FreeCAD channel",
                "interfaces": ["CLI", "official MCP stdio client"],
                "packages": {"cli": binary, "runtime": runtime, "mcp": mcp},
                "results": results,
                "cad_inspection": imports,
                "physical_validation": "unqualified",
            }
            (root / "verification.json").write_text(
                json.dumps(report, indent=2, allow_nan=False)
            )
            print(json.dumps(report, indent=2, allow_nan=False))
        finally:
            worker.terminate()
            worker.wait(timeout=5)
            for unit in owned:
                subprocess.run(
                    ["systemctl", "--user", "stop", unit], check=False, timeout=15
                )


if __name__ == "__main__":
    main()
