"""Exact-package prescribed CPU thermal CLI/MCP, immutable histories and lifecycle gate."""

import argparse
import asyncio
import hashlib
import importlib.util
import json
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


def load_bridge(name):
    path = Path(__file__).resolve().parents[1] / f"adapters/{name}.py"
    source = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(source)
    source.loader.exec_module(module)
    return module


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
        for k in ("cad", "openlb", "render", "video", "filter", "fem")
    ):
        raise ValueError("CPU thermal-only operation runtime required")
    reference = json.loads((args.native_reference / "verification.json").read_text())
    if native["thermal"] != reference["adapter"]:
        raise ValueError("worker must use the exact native-qualified thermal adapter")
    fem, thermal = load_bridge("fem_reference"), load_bridge("thermal_history")
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    state, endpoint, profile = (
        root / "state",
        root / "state/worker.sock",
        root / "profile.json",
    )
    profile.write_text(
        json.dumps(
            {
                "schema_version": 1,
                "policy": "research",
                "allowed_input_root": str(root),
                "max_ram_bytes": 2 * 1024**3,
                "max_disk_bytes": 1024**3,
                "threads": 1,
                "timeout_seconds": 240,
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
                raise RuntimeError("worker startup failed; inspect retained worker.log")
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
        return command("case", "plan-thermal-reference", path)

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
            reply = await client.call_tool(
                "case_plan_thermal_reference", {"spec": spec}
            )
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

    def fixture(recipe):
        candidates = [
            v["request"]
            for v in reference["results"]
            if v["recipe"] == recipe
            and v["request"]["resolution"] == 8
            and v["request"]["max_step_s"] == 0.5
        ]
        if len(candidates) != 1:
            raise ValueError("unique native-qualified prescribed recipe required")
        return candidates[0]

    try:
        with (root / "worker.log").open("xb") as log:
            worker = start(log)
            for interface, recipe in [
                ("cli", "adiabatic_heater"),
                ("mcp", "cold_restart_robin"),
            ]:
                spec = fixture(recipe)
                key = f"thermal-{interface}"
                plan = planned(spec, key)
                assert (
                    plan["plan"]["schema_version"] == 6
                    and plan["plan"]["thermal"] == spec
                )
                assert "case" not in plan["plan"] and "fem" not in plan["plan"]
                assert (
                    plan["plan"]["observation"]["retained_times_s"]
                    == spec["observation_times_s"]
                )
                assert (
                    submit(plan, "wrong-approval", "a" * 64, allow_error=True)["ok"]
                    is False
                )
                changed = planned(
                    {**spec, "heater_history": [[0.0, 0.0], [60.0, 0.0], [120.0, 2.0]]},
                    f"changed-{interface}",
                )
                assert changed["approval_digest"] != plan["approval_digest"]
                assert (
                    submit(
                        changed,
                        "changed-history",
                        plan["approval_digest"],
                        allow_error=True,
                    )["ok"]
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
                    == "harbor-cad-thermal-cpu-v1"
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
                    str(binary), endpoint, job["id"], {"succeeded"}, timeout=250
                )
                assert outcome["invocation_id"] == running["invocation_id"]
                wait_admission_release(state, job)
                wait_retention_release(state, job)
                bundle = root / f"bundle-{interface}"
                command("artifact", "export", "--state", state, job["id"], bundle)
                records = verify_manifest(bundle)
                data = bundle / "stages/thermal"
                receipt = json.loads((data / "thermal-receipt.json").read_text())
                mesh = json.loads((data / "mesh.json").read_text())
                assert (
                    mesh["coordinate_unit"] == "m"
                    and mesh["positive_gauss_jacobians"] is True
                )
                raw = (data / "reference.dat").read_bytes()
                assert receipt["native_field_sha256"] == hashlib.sha256(raw).hexdigest()
                assert (
                    receipt["mesh_sha256"]
                    == hashlib.sha256((data / "mesh.json").read_bytes()).hexdigest()
                )
                nodes = {int(k): v for k, v in mesh["nodes"].items()}
                cells = {int(k): v for k, v in mesh["elements"].items()}
                checks, metrics, fields = thermal.verify(
                    spec, nodes, cells, fem.read_dat(raw.decode())
                )
                assert checks == receipt["numerical_verification"]
                assert (
                    json.loads((data / "thermal-metrics.json").read_text())["metrics"]
                    == metrics
                )
                stored_fields = json.loads((data / "thermal-fields.json").read_text())
                assert (
                    stored_fields["unit"] == "K"
                    and stored_fields["association"] == "point"
                )
                assert stored_fields["times"] == json.loads(json.dumps(fields))
                assert receipt["moisture_risk"] == spec["moisture_risk"]
                assert receipt["maximum_native_step_s"] == thermal.integration_step(
                    spec
                )
                assert receipt["energy_output_times_s"] == thermal.output_times(spec)
                assert receipt["physical_times_s"] == spec["observation_times_s"]
                assert (
                    receipt["sandbox"]["policy"] == "harbor-cad-thermal-cpu-v1"
                    and len(receipt["sandbox"]["checks"]) == 8
                )
                assert all(receipt["sandbox"]["checks"].values())
                request = (bundle / "native-thermal-request.json").read_bytes()
                assert (
                    json.loads(request) == spec
                    and receipt["request_sha256"] == hashlib.sha256(request).hexdigest()
                )
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
                    capability["formulation"] == "plane_wall_robin"
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
                plan = planned(fixture("cold_restart_robin"), action)
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
            "scope": "synthetic prescribed CPU transient heat transfer, sandbox, immutable fields/histories and owned lifecycle",
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
