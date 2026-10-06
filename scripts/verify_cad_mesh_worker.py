"""Exact-package source-bound CAD mesh CLI/MCP, retained-source and lifecycle gate."""

import argparse
import asyncio
import hashlib
import json
import os
import shutil
import socket
import sqlite3
import subprocess
import time
from pathlib import Path

from verify_cad_mesh import verify_box_mesh
from verify_native_cpu import verify_manifest
from verify_systemd import (
    admission_record,
    retention_snapshot,
    wait_admission_release,
    wait_job,
    wait_retention_release,
)


def checksum(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in (
        "executable",
        "runtime",
        "mcp",
        "authority",
        "cad-reference",
        "output",
    ):
        parser.add_argument(f"--{name}", type=Path, required=True)
    args = parser.parse_args()
    binary, runtime, mcp = (
        p.resolve(strict=True) for p in (args.executable, args.runtime, args.mcp)
    )
    if any(
        not p.is_relative_to("/nix/store") or not p.is_file()
        for p in (binary, runtime, mcp)
    ):
        raise ValueError("immutable exact CLI/runtime/MCP packages required")
    native = json.loads(runtime.read_text())
    if not native.get("cad_mesh_closure") or any(
        native.get(k)
        for k in ("cad", "openlb", "fem", "thermal", "render", "video", "filter")
    ):
        raise ValueError("CPU imported-mesh-only closure runtime required")
    reference_path = args.cad_reference / "verification.json"
    reference = json.loads(reference_path.read_text())
    if len(reference["results"]) != 6 or len(reference["rejections"]) != 7:
        raise ValueError(
            "complete origin/translated CAD qualification prerequisite required"
        )
    sources = {r["fixture"]: r["job"]["id"] for r in reference["results"]}
    original_state = args.cad_reference / "state"
    original_hashes = {
        label: checksum(original_state / "artifacts" / job / "solid.brep")
        for label, job in sources.items()
    }
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    state, endpoint = root / "state", root / "state/worker.sock"
    state.mkdir(mode=0o700)
    # A closed-state backup permits deliberate mutation tests on distinct copies
    # while preserving the exact source jobs, approvals and original evidence.
    with sqlite3.connect(
        f"file:{original_state / 'jobs.sqlite3'}?mode=ro", uri=True
    ) as source:
        if source.execute(
            "SELECT count(*) FROM jobs WHERE state NOT IN ('succeeded','failed','cancelled')"
        ).fetchone()[0]:
            raise ValueError("source-state backup requires all jobs terminal")
        with sqlite3.connect(state / "jobs.sqlite3") as destination:
            source.backup(destination)
    for name in ("artifacts", "profiles"):
        shutil.copytree(original_state / name, state / name)
    profile = root / "profile.json"
    profile.write_text(
        json.dumps(
            {
                "schema_version": 1,
                "policy": "research",
                "allowed_input_root": str(root),
                "max_ram_bytes": 4 * 1024**3,
                "max_disk_bytes": 2 * 1024**3,
                "threads": 2,
                "timeout_seconds": 180,
                "native_runtime": str(runtime),
                "service_mode": "systemd",
            }
        )
    )
    environment = {
        k: os.environ[k]
        for k in ("HOME", "XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS", "PATH")
        if k in os.environ
    }
    worker, owned, results = None, [], []

    def command(*argv, allow_error=False):
        process = subprocess.run(
            [str(binary), *map(str, argv)],
            env=environment,
            capture_output=True,
            timeout=30,
            check=False,
        )
        reply = json.loads(process.stdout)
        if process.returncode and not allow_error:
            raise RuntimeError(reply)
        return reply

    def start(log):
        process = subprocess.Popen(
            [
                str(binary),
                "worker",
                "--state",
                str(state),
                "--profile",
                str(profile),
                "--authority",
                str(args.authority),
            ],
            env=environment,
            stdout=log,
            stderr=subprocess.STDOUT,
        )
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise RuntimeError("worker startup failed; inspect worker.log")
            if endpoint.exists():
                try:
                    with socket.socket(socket.AF_UNIX) as connection:
                        connection.settimeout(1)
                        connection.connect(str(endpoint))
                        connection.sendall(
                            b'{"protocol_version":1,"request_id":"ready","request":{"operation":"doctor"}}\n'
                        )
                        if json.loads(connection.recv(65536))["ok"]:
                            return process
                except OSError:
                    pass
            time.sleep(0.01)
        process.terminate()
        process.wait(timeout=5)
        raise TimeoutError("worker startup")

    def planned(label, key, resolution=8, allow_error=False):
        request = {
            "source_job": sources[label],
            "region_name": "solid",
            "resolution": resolution,
            "geometry_tolerance_m": 1e-6,
        }
        path = root / f"request-{key}.json"
        path.write_text(json.dumps(request))
        reply = command(
            "--socket", endpoint, "cad", "mesh", path, allow_error=allow_error
        )
        return reply if allow_error else reply["data"]

    def submit(plan, key, approval=None, allow_error=False):
        path = root / f"plan-{key}.json"
        path.write_text(json.dumps(plan["plan"]))
        reply = command(
            "--socket",
            endpoint,
            "job",
            "submit",
            path,
            "--approve",
            approval or plan["approval_digest"],
            "--idempotency-key",
            key,
            allow_error=allow_error,
        )
        return reply if allow_error else reply["data"]

    async def mcp_submit(label, plan, key):
        from mcp import Client
        from mcp.client.stdio import StdioServerParameters

        params = StdioServerParameters(
            command=str(mcp),
            args=["--profile", "cad"],
            env={**environment, "HARBOR_CAD_SOCKET": str(endpoint)},
        )
        async with Client(params) as client:
            reply = await client.call_tool(
                "cad_plan_mesh",
                {
                    "request_spec": {
                        "source_job": sources[label],
                        "region_name": "solid",
                        "resolution": 8,
                        "geometry_tolerance_m": 1e-6,
                    }
                },
            )
            assert not reply.is_error and reply.structured_content == plan
            reply = await client.call_tool(
                "cad_mesh_submit",
                {
                    "plan": plan["plan"],
                    "approved_digest": plan["approval_digest"],
                    "idempotency_key": key,
                },
            )
            assert not reply.is_error
            return reply.structured_content

    try:
        with (root / "worker.log").open("xb") as log:
            worker = start(log)
            for interface, label in (("cli", "origin"), ("mcp", "translated")):
                key = f"mesh-{interface}"
                plan = planned(label, key)
                assert (
                    plan["plan"]["schema_version"] == 7
                    and "case" not in plan["plan"]
                    and "fem" not in plan["plan"]
                )
                source = state / "artifacts" / sources[label] / "solid.brep"
                original = source.read_bytes()
                source.write_bytes(original + b"source mutation before submission")
                assert not planned(
                    label, f"reject-source-{interface}", allow_error=True
                )["ok"]
                assert not submit(
                    plan, f"reject-stale-source-{interface}", allow_error=True
                )["ok"]
                source.write_bytes(original)
                assert not submit(
                    plan, f"reject-approval-{interface}", "a" * 64, allow_error=True
                )["ok"]
                changed = json.loads(json.dumps(plan))
                changed["plan"]["cad_source"]["geometry"]["source_transform"][3] += 1.0
                assert not submit(
                    changed, f"reject-transform-{interface}", allow_error=True
                )["ok"]
                job = (
                    submit(plan, key)
                    if interface == "cli"
                    else asyncio.run(mcp_submit(label, plan, key))
                )
                owned.append(job["unit"])
                retained = state / "artifacts" / job["id"] / "retained-cad/solid.brep"
                assert (
                    retained.read_bytes() == original
                    and retained.stat().st_ino != source.stat().st_ino
                )
                source.write_bytes(original + b"original mutation after acknowledgment")
                running = wait_job(str(binary), endpoint, job["id"], {"running"})
                retention = retention_snapshot(state, job, str(binary))
                assert (
                    retention["intent"]["binding"]["sandbox_policy"]
                    == "harbor-cad-cad-mesh-cpu-v1"
                )
                reservation = admission_record(state, job)
                assert (
                    reservation is not None
                    and reservation["cards"] == {}
                    and reservation["ram"] == plan["plan"]["stages"][0]["ram_bytes"]
                )
                worker.kill()
                worker.wait(timeout=5)
                worker = start(log)
                assert submit(plan, key)["id"] == job["id"]
                outcome = wait_job(
                    str(binary), endpoint, job["id"], {"succeeded"}, timeout=180
                )
                assert outcome["invocation_id"] == running["invocation_id"]
                source.write_bytes(original)
                wait_admission_release(state, job)
                wait_retention_release(state, job)
                bundle = root / f"bundle-{interface}"
                command("artifact", "export", "--state", state, job["id"], bundle)
                records = verify_manifest(bundle)
                data = bundle / "stages/mesh"
                receipt = json.loads((data / "cad-mesh-receipt.json").read_text())
                spec = plan["plan"]["cad_source"]["geometry"]
                checked = verify_box_mesh(
                    spec, json.loads((data / "mesh.json").read_text())
                )
                assert checked["passed"] and receipt["mesh_sha256"] == checksum(
                    data / "mesh.json"
                )
                assert receipt["brep_sha256"] == checksum(
                    bundle / "retained-cad/solid.brep"
                )
                assert receipt["request_sha256"] == checksum(
                    bundle / "native-cad-mesh-request.json"
                )
                assert (
                    receipt["source_transform"] == spec["source_transform"]
                    and receipt["world_bounds_m"] == spec["bounds_m"]
                )
                assert (
                    receipt["sandbox"]["policy"] == "harbor-cad-cad-mesh-cpu-v1"
                    and len(receipt["sandbox"]["checks"]) == 10
                    and all(receipt["sandbox"]["checks"].values())
                )
                resources = json.loads((bundle / "service-resources.json").read_text())
                assert (
                    resources["invocation"] == running["invocation_id"]
                    and resources["kernel_resources"]["controls"]["cpu.max"]
                    == "200000 100000"
                )
                assert resources["kernel_resources"]["aggregate_memory_peak_bytes"] > 0
                historical = command(
                    "--socket", endpoint, "qualify", "--job", job["id"]
                )["data"]
                capability = historical["capabilities"][0]
                assert (
                    capability["runtime_execution"] == "recorded"
                    and capability["numerical_verification"] == "reported_pass"
                )
                assert (
                    capability["formulation"] == "imported_axis_aligned_box"
                    and historical["physical_validation"] == "unqualified"
                )
                results.append(
                    {
                        "interface": interface,
                        "source_fixture": label,
                        "job": outcome,
                        "receipt": receipt,
                        "independent_mesh_check": checked,
                        "records": len(records),
                        "service_resources": resources,
                        "admission_record": reservation,
                        "historical_evidence": historical,
                        "restart": "same invocation",
                        "source_mutation": "pre-submission rejected; acknowledged distinct copies survived",
                        "reservation_and_roots_released": True,
                    }
                )
            for action in ("forced-death", "cancel"):
                plan = planned("translated", action, resolution=32)
                job = submit(plan, action)
                owned.append(job["unit"])
                wait_job(str(binary), endpoint, job["id"], {"running"})
                retained = retention_snapshot(state, job, str(binary))
                if action == "forced-death":
                    worker.kill()
                    worker.wait(timeout=5)
                    subprocess.run(
                        [
                            "systemctl",
                            "--user",
                            "kill",
                            "--signal=KILL",
                            "--kill-whom=all",
                            job["unit"],
                        ],
                        check=True,
                    )
                    worker = start(log)
                    outcome = wait_job(str(binary), endpoint, job["id"], {"failed"})
                else:
                    command("--socket", endpoint, "job", "cancel", job["id"])
                    outcome = wait_job(str(binary), endpoint, job["id"], {"cancelled"})
                assert submit(plan, action)["id"] == job["id"]
                wait_admission_release(state, job)
                wait_retention_release(state, job)
                results.append(
                    {
                        "action": action,
                        "job": outcome,
                        "active_retention": retained,
                        "retry": "same terminal job",
                        "reservation_and_roots_released": True,
                    }
                )
        assert original_hashes == {
            label: checksum(original_state / "artifacts" / job / "solid.brep")
            for label, job in sources.items()
        }
        report = {
            "schema_version": 1,
            "binary": str(binary),
            "runtime": str(runtime),
            "mcp": str(mcp),
            "runtime_sha256": checksum(runtime),
            "cad_reference": str(reference_path),
            "cad_reference_sha256": checksum(reference_path),
            "original_source_hashes": original_hashes,
            "results": results,
            "physical_validation": "unqualified",
            "scope": "registered original CAD to isolated durable CPU imported mesh; geometric correspondence and lifecycle only",
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
