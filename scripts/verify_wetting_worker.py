"""Exact approved CPU wetting CLI/MCP, original-field, isolation and lifecycle gate."""

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
from verify_openlb_hip import service_resources
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
        raise ValueError("exact immutable packaged CLI/runtime/MCP required")
    native = json.loads(runtime.read_text())
    reference = json.loads((args.native_reference / "verification.json").read_text())
    standalone = Path(reference["runtime"])
    if (
        checksum(standalone) != reference["runtime_sha256"]
        or native["wetting"] != json.loads(standalone.read_text())["wetting"]
        or len(reference["results"]) != 6
        or len(reference["rejections"]) != 10
        or len(reference["refinements"]) != 2
        or not all(v["passed"] for v in reference["refinements"])
    ):
        raise ValueError(
            "worker requires exact complete native mass/angle/settling/refinement-qualified wetting adapter"
        )
    if any(
        native.get(key) is not None
        for key in (
            "cad",
            "openlb",
            "render",
            "video",
            "filter",
            "fem",
            "thermal",
            "cad_mesh",
            "fem_imported",
        )
    ):
        raise ValueError("operation-specific CPU wetting runtime required")
    source = importlib.util.spec_from_file_location(
        "wetting", Path(__file__).resolve().parents[1] / "adapters/wetting_reference.py"
    )
    bridge = importlib.util.module_from_spec(source)
    source.loader.exec_module(bridge)
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
                "max_disk_bytes": 2 * 1024**3,
                "threads": 2,
                "timeout_seconds": 500,
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
    before = service_resources()

    def command(*argv, allow_error=False):
        process = subprocess.run(
            [str(binary), *map(str, argv)],
            env=environment,
            capture_output=True,
            timeout=30,
            check=False,
        )
        reply = json.loads(process.stdout)
        if not allow_error and process.returncode:
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
                str(args.authority.resolve(strict=True)),
            ],
            env=environment,
            stdout=log,
            stderr=subprocess.STDOUT,
        )
        try:
            deadline = time.monotonic() + 15
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
                time.sleep(0.02)
            raise TimeoutError("worker startup")
        except BaseException:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=5)
            raise

    def fixture(angle, resolution):
        candidates = [
            v["request"]
            for v in reference["results"]
            if v["angle"] == angle and v["resolution"] == resolution
        ]
        if len(candidates) != 1:
            raise ValueError("unique exact native-qualified wetting reference required")
        return candidates[0]

    def planned(spec, key):
        path = root / f"spec-{key}.json"
        path.write_text(json.dumps(spec))
        return command("case", "plan-wetting-reference", path)

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
                "case_plan_wetting_reference", {"spec": spec}
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

    try:
        with (root / "worker.log").open("xb") as log:
            worker = start(log)
            for interface, angle, resolution in (("cli", 90, 24), ("mcp", 100, 36)):
                spec = fixture(angle, resolution)
                key = f"wetting-{interface}"
                plan = planned(spec, key)
                assert (
                    plan["plan"]["schema_version"] == 9
                    and plan["plan"]["wetting"] == spec
                    and "case" not in plan["plan"]
                )
                dt = bridge.validate(spec)["physical_step_s"]
                assert all(
                    math.isclose(t, s * dt, rel_tol=1e-12, abs_tol=1e-18)
                    for t, s in zip(
                        plan["plan"]["observation"]["retained_times_s"],
                        spec["observation_steps"],
                        strict=True,
                    )
                )
                rejected = submit(plan, "wrong-approval", "a" * 64, allow_error=True)
                assert rejected["ok"] is False
                changed = planned(
                    {**spec, "contact_angle_deg": spec["contact_angle_deg"] + 1},
                    f"changed-{key}",
                )
                assert (
                    changed["approval_digest"] != plan["approval_digest"]
                    and submit(changed, "changed-angle", plan["approval_digest"], True)[
                        "ok"
                    ]
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
                assert retention["intent"]["binding"]["sandbox_policy"] == bridge.POLICY
                reservation = admission_record(state, job)
                assert (
                    reservation is not None
                    and reservation["ram"]
                    == max(s["ram_bytes"] for s in plan["plan"]["stages"])
                    and reservation["cards"] == {}
                )
                worker.kill()
                worker.wait(timeout=5)
                worker = start(log)
                assert submit(plan, key)["id"] == job["id"]
                outcome = wait_job(
                    str(binary), endpoint, job["id"], {"succeeded"}, timeout=510
                )
                assert outcome["invocation_id"] == running["invocation_id"]
                wait_admission_release(state, job)
                wait_retention_release(state, job)
                bundle = root / f"bundle-{interface}"
                command("artifact", "export", "--state", state, job["id"], bundle)
                records = verify_manifest(bundle)
                data = bundle / "stages/wetting"
                receipt = json.loads(
                    (data / "verified-wetting-receipt.json").read_text()
                )
                raw = (bundle / "native-wetting-request.json").read_bytes()
                assert (
                    json.loads(raw) == spec
                    and receipt["request_sha256"] == hashlib.sha256(raw).hexdigest()
                )
                assert (
                    bundle / "native-runtime.json"
                ).read_bytes() == runtime.read_bytes()
                assert (
                    receipt["sandbox"]["policy"] == bridge.POLICY
                    and len(receipt["sandbox"]["checks"]) == 8
                    and all(receipt["sandbox"]["checks"].values())
                )
                rechecked = []
                for item in receipt["independent_fields"]:
                    field = data / item["path"]
                    check = bridge.assess_field(spec, field.read_bytes())
                    assert check == item["check"] and checksum(field) == item["sha256"]
                    rechecked.append({**item, "check": check})
                checks = bridge.verify(spec, rechecked)
                assert (
                    checks == receipt["numerical_verification"]
                    and checks["mass_passed"]
                    and checks["angle_passed"]
                )
                assert (
                    abs(
                        rechecked[-1]["check"]["contact_angle_deg"]
                        - rechecked[-2]["check"]["contact_angle_deg"]
                    )
                    <= 0.2
                )
                historical = command(
                    "--socket", endpoint, "qualify", "--job", job["id"]
                )["data"]
                capability = historical["capabilities"][0]
                assert (
                    capability["runtime_execution"] == "recorded"
                    and capability["numerical_verification"] == "reported_pass"
                    and capability["dimensions"] == 2
                    and capability["convergence"] == "not_assessed"
                )
                assert (
                    historical["physical_validation"] == "unqualified"
                    and historical["current_runtime_qualification"] == "not_assessed"
                )
                resources = json.loads((bundle / "service-resources.json").read_text())
                assert (
                    resources["job_id"] == job["id"]
                    and resources["invocation"] == running["invocation_id"]
                )
                assert (
                    resources["kernel_resources"]["controls"]["cpu.max"]
                    == "200000 100000"
                    and resources["kernel_resources"]["aggregate_memory_peak_bytes"] > 0
                )
                results.append(
                    {
                        "interface": interface,
                        "job": outcome,
                        "receipt": receipt,
                        "independent_raw_fields": rechecked,
                        "independent_checks": checks,
                        "records": len(records),
                        "historical_evidence": historical,
                        "admission_record": reservation,
                        "service_resources": resources,
                        "restart": "same invocation",
                        "approval_mutations_rejected": True,
                        "reservation_and_roots_released": True,
                    }
                )
            for action in ("forced-death", "cancel"):
                plan = planned(fixture(100, 48), action)
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
    finally:
        for unit in owned:
            subprocess.run(
                ["systemctl", "--user", "stop", unit], check=False, capture_output=True
            )
        if worker is not None and worker.poll() is None:
            worker.terminate()
            worker.wait(timeout=5)
    report = {
        "schema_version": 1,
        "binary": str(binary),
        "binary_sha256": checksum(binary),
        "runtime": str(runtime),
        "runtime_sha256": checksum(runtime),
        "mcp": str(mcp),
        "mcp_sha256": checksum(mcp),
        "native_reference_sha256": checksum(
            args.native_reference / "verification.json"
        ),
        "results": results,
        "service_resources_before": before,
        "service_resources_after": service_resources(),
        "physical_validation": "unqualified",
        "scope": "approved synthetic equal-property planar CPU wetting CLI/MCP, exact original fields and native times, isolated closure, worker restart/idempotency, complete-tree forced death/cancellation and release",
    }
    (root / "verification.json").write_text(
        json.dumps(report, indent=2, allow_nan=False)
    )
    print(
        json.dumps(
            {
                "report_sha256": checksum(root / "verification.json"),
                "results": len(results),
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
