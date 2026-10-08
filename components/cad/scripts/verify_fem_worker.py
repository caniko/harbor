"""Opt-in exact-package CPU FEM CLI/MCP, sandbox, fields and owned-lifecycle gate."""

import argparse
import asyncio
import hashlib
import importlib.util
import json
import math
import os
import socket
import subprocess
import time
from pathlib import Path

from verify_native_cpu import verify_manifest
from verify_systemd import (
    admission_record,
    retention_snapshot,
    wait_admission_release,
    wait_job,
    wait_retention_release,
)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in (
        "executable",
        "runtime",
        "mcp",
        "authority",
        "native-reference",
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
        raise ValueError("immutable packaged CLI/runtime/MCP required")
    native = json.loads(runtime.read_text())
    if any(
        native.get(k) is not None
        for k in ("cad", "openlb", "render", "video", "filter")
    ):
        raise ValueError("CPU FEM-only operation runtime required")
    reference = json.loads((args.native_reference / "verification.json").read_text())
    reference_runtime = json.loads(Path(reference["runtime"]).read_text())
    if native["fem"] != reference_runtime["fem"]:
        raise ValueError("worker must use the exact native-qualified FEM adapter")
    module_path = Path(__file__).resolve().parents[1] / "adapters/fem_reference.py"
    module_spec = importlib.util.spec_from_file_location(
        "fem_reference_gate", module_path
    )
    bridge = importlib.util.module_from_spec(module_spec)
    module_spec.loader.exec_module(bridge)
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    state, endpoint = root / "state", root / "state/worker.sock"
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
    authority = args.authority.resolve(strict=True)
    environment = {
        k: os.environ[k]
        for k in ("HOME", "XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS", "PATH")
        if k in os.environ
    }
    worker, owned, results = None, [], []

    def command(*argv, allow_error=False):
        result = subprocess.run(
            [str(binary), *map(str, argv)],
            env=environment,
            capture_output=True,
            timeout=30,
            check=False,
        )
        reply = json.loads(result.stdout)
        if not allow_error and result.returncode:
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
                str(authority),
            ],
            env=environment,
            stdout=log,
            stderr=subprocess.STDOUT,
        )
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise RuntimeError("worker failed; inspect worker.log")
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

    def planned(spec, key):
        path = root / f"spec-{key}.json"
        path.write_text(json.dumps(spec))
        return command("case", "plan-fem-reference", path)

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

    async def mcp_submit(spec, plan, key):
        from mcp import Client
        from mcp.client.stdio import StdioServerParameters

        params = StdioServerParameters(
            command=str(mcp),
            args=["--profile", "simulation"],
            env={**environment, "HARBOR_CAD_SOCKET": str(endpoint)},
        )
        async with Client(params) as client:
            reply = await client.call_tool("case_plan_fem_reference", {"spec": spec})
            assert not reply.is_error and reply.structured_content == plan
            reply = await client.call_tool(
                "job_submit",
                {
                    "plan": plan["plan"],
                    "approved_digest": plan["approval_digest"],
                    "idempotency_key": key,
                },
            )
            assert not reply.is_error
            return reply.structured_content

    def fixture(mode, resolution):
        spec = {
            "schema_version": 1,
            "synthetic": True,
            "backend": "cpu",
            "mode": mode,
            "size_m": [0.02, 0.01, 0.01],
            "resolution": resolution,
            "geometry_tolerance_m": 1e-6,
            "temperatures_k": [293.15, 303.15],
            "numerical_tolerance": 1e-6,
        }
        spec.update(
            {"conductivity_w_m_k": 20.0}
            if mode == "thermal_boundary"
            else {
                "young_modulus_pa": 200e9,
                "poisson_ratio": 0.3,
                "expansion_per_k": 12e-6,
            }
        )
        return spec

    try:
        with (root / "worker.log").open("xb") as log:
            worker = start(log)
            for interface, mode in [
                ("cli", "thermal_boundary"),
                ("mcp", "free_expansion"),
            ]:
                spec = fixture(mode, 8)
                key = f"fem-{interface}"
                plan = planned(spec, key)
                assert (
                    plan["plan"]["schema_version"] == 5 and "case" not in plan["plan"]
                )
                assert plan["plan"]["observation"]["retained_times_s"] == []
                assert (
                    submit(plan, "wrong-approval", "a" * 64, allow_error=True)["ok"]
                    is False
                )
                job = (
                    submit(plan, key)
                    if interface == "cli"
                    else asyncio.run(mcp_submit(spec, plan, key))
                )
                owned.append(job["unit"])
                running = wait_job(str(binary), endpoint, job["id"], {"running"})
                retention = retention_snapshot(state, job, str(binary))
                assert (
                    retention["intent"]["binding"]["sandbox_policy"]
                    == "harbor-cad-fem-cpu-v1"
                )
                reservation = admission_record(state, job)
                assert (
                    reservation is not None
                    and reservation["ram"] == 2 * 1024**3
                    and reservation["cards"] == {}
                )
                worker.kill()
                worker.wait(timeout=5)
                worker = start(log)
                assert submit(plan, key)["id"] == job["id"]
                outcome = wait_job(
                    str(binary), endpoint, job["id"], {"succeeded"}, timeout=190
                )
                assert outcome["invocation_id"] == running["invocation_id"]
                wait_admission_release(state, job)
                wait_retention_release(state, job)
                bundle = root / f"bundle-{interface}"
                command("artifact", "export", "--state", state, job["id"], bundle)
                records = verify_manifest(bundle)
                data_root = bundle / "stages/fem"
                receipt = json.loads(
                    (data_root / "fem-reference-receipt.json").read_text()
                )
                mesh = json.loads((data_root / "mesh.json").read_text())
                assert (
                    mesh["coordinate_unit"] == "m"
                    and mesh["positive_gauss_jacobians"] is True
                )
                assert math.isclose(
                    mesh["integrated_volume_m3"],
                    math.prod(spec["size_m"]),
                    rel_tol=1e-10,
                )
                assert (
                    receipt["mesh_sha256"]
                    == hashlib.sha256(
                        (data_root / "mesh.json").read_bytes()
                    ).hexdigest()
                )
                raw = (data_root / "reference.dat").read_bytes()
                assert receipt["native_field_sha256"] == hashlib.sha256(raw).hexdigest()
                nodes = {int(k): v for k, v in mesh["nodes"].items()}
                cells = {int(k): v for k, v in mesh["elements"].items()}
                checks = bridge.verify(
                    spec, nodes, cells, bridge.read_dat(raw.decode())
                )
                assert checks == receipt["numerical_verification"]
                assert receipt["sandbox"]["policy"] == "harbor-cad-fem-cpu-v1"
                assert all(receipt["sandbox"]["checks"].values())
                native_spec = (bundle / "native-fem-request.json").read_bytes()
                assert (
                    receipt["request_sha256"] == hashlib.sha256(native_spec).hexdigest()
                )
                assert json.loads(native_spec) == spec
                assert (
                    bundle / "native-runtime.json"
                ).read_bytes() == runtime.read_bytes()
                resources = json.loads((bundle / "service-resources.json").read_text())
                assert (
                    resources["job_id"] == job["id"]
                    and resources["invocation"] == running["invocation_id"]
                )
                assert resources["kernel_resources"]["aggregate_memory_peak_bytes"] > 0
                assert (
                    resources["kernel_resources"]["cpu_stat_microseconds_and_counts"][
                        "usage_usec"
                    ]
                    > 0
                )
                historical = command(
                    "--socket", endpoint, "qualify", "--job", job["id"]
                )["data"]
                capability = historical["capabilities"][0]
                assert (
                    capability["runtime_execution"] == "recorded"
                    and capability["numerical_verification"] == "reported_pass"
                )
                assert (
                    capability["formulation"] == mode
                    and capability["requested_device"] is None
                )
                assert (
                    historical["physical_validation"] == "unqualified"
                    and historical["current_runtime_qualification"] == "not_assessed"
                )
                results.append(
                    {
                        "interface": interface,
                        "job": outcome,
                        "receipt": receipt,
                        "independent_analytical_recheck": checks,
                        "records": len(records),
                        "service_resources": resources,
                        "admission_record": reservation,
                        "restart": "same invocation",
                        "historical_evidence": historical,
                        "reservation_and_roots_released": True,
                    }
                )
            for action in ("forced-death", "cancel"):
                plan = planned(fixture("free_expansion", 16), action)
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
        report = {
            "schema_version": 1,
            "binary": str(binary),
            "runtime": str(runtime),
            "mcp": str(mcp),
            "runtime_sha256": hashlib.sha256(runtime.read_bytes()).hexdigest(),
            "results": results,
            "physical_validation": "unqualified",
            "scope": "synthetic static CPU FEM, sandbox, field/mesh integrity and owned worker lifecycle",
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
