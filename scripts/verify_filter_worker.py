"""Packaged CLI/MCP source-bound HIP filtering and owned lifecycle qualification."""

import argparse
import asyncio
import hashlib
import json
import os
import socket
import sqlite3
import subprocess
import time
from pathlib import Path

from verify_filter_hip import read_gradient
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
        "source-state",
        "native-reference",
        "output",
    ):
        parser.add_argument(f"--{name}", type=Path, required=True)
    parser.add_argument("--source-job", required=True)
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
    if native.get("cad") is not None or any(
        native.get(k) is not None
        for k in ("render", "video", "openlb", "openlb_hip", "openlb_cuda")
    ):
        raise ValueError("compute-only filter runtime required")
    reference = json.loads((args.native_reference / "verification.json").read_text())
    if reference["runtime_sha256"] != hashlib.sha256(runtime.read_bytes()).hexdigest():
        raise ValueError(
            "qualified native reference must use the same exact filter runtime"
        )
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    state = root / "state"
    state.mkdir(mode=0o700)
    original = args.source_state.resolve(strict=True) / "artifacts" / args.source_job
    with sqlite3.connect(
        f"file:{args.source_state / 'jobs.sqlite3'}?mode=ro", uri=True
    ) as old:
        if old.execute(
            "SELECT count(*) FROM jobs WHERE state IN ('queued','starting','running','cancelling')"
        ).fetchone()[0]:
            raise ValueError("closed source state required")
        if old.execute(
            "SELECT state FROM jobs WHERE id=?", (args.source_job,)
        ).fetchone() != ("succeeded",):
            raise ValueError("succeeded source required")
        with sqlite3.connect(state / "jobs.sqlite3") as new:
            old.backup(new)
            new.execute("DELETE FROM jobs WHERE id != ?", (args.source_job,))
            for table in (
                "events",
                "artifacts",
                "job_profiles",
                "job_executions",
                "job_authorizations",
            ):
                new.execute(f"DELETE FROM {table} WHERE job != ?", (args.source_job,))
        manifests = [
            json.loads(r[0])
            for r in old.execute(
                "SELECT manifest FROM artifacts WHERE job=?", (args.source_job,)
            )
        ]
    if sum(r["bytes"] for r in manifests) > 128 * 1024**2:
        raise ValueError("bounded source qualification fixture required")
    copied = state / "artifacts" / args.source_job
    for record in manifests:
        relative = Path(record["path"])
        if relative.is_absolute() or ".." in relative.parts:
            raise ValueError("source path escape")
        source = original / relative
        if any(
            p.is_symlink()
            for p in (source, *source.parents)
            if p.is_relative_to(original)
        ):
            raise ValueError("source component symlink")
        data = source.read_bytes()
        if (
            len(data) != record["bytes"]
            or hashlib.sha256(data).hexdigest() != record["sha256"]
        ):
            raise ValueError("source artifact differs from registered bytes")
        target = copied / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open("xb") as stream:
            stream.write(data)
    snapshot = json.loads((copied / "retained-fields/snapshot.json").read_text())
    time_record = snapshot["times"][-1]
    authority_path = args.authority.resolve(strict=True)
    authority = json.loads(authority_path.read_text())
    choices = [
        d
        for d in authority["allowed_devices"]
        if d["role"] == "compute" and d["backend"] == "hip"
    ]
    if len(choices) != 1:
        raise ValueError("one explicit authorized HIP fixture required")
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
                "timeout_seconds": 120,
                "native_runtime": str(runtime),
                "service_mode": "systemd",
            }
        )
    )
    endpoint = state / "worker.sock"
    environment = {
        k: os.environ[k]
        for k in ("HOME", "XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS", "PATH")
        if k in os.environ
    }
    worker, owned, results = None, [], []

    def command(*values, allow_error=False):
        process = subprocess.run(
            [str(binary), *map(str, values)],
            env=environment,
            capture_output=True,
            timeout=30,
            check=False,
        )
        response = json.loads(process.stdout)
        if not allow_error and process.returncode:
            raise RuntimeError(response)
        return response

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
                str(authority_path),
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

    def request(field="velocity"):
        return {
            "source_job": args.source_job,
            "filter": {"time_s": time_record["requested_s"], "field": field},
            "compute": choices[0],
        }

    def planned(spec, key):
        path = root / f"request-{key}.json"
        path.write_text(json.dumps(spec))
        return command("--socket", endpoint, "filter", path)["data"]

    def submit(plan, key, allow_error=False):
        path = root / f"plan-{key}.json"
        path.write_text(json.dumps(plan["plan"]))
        response = command(
            "--socket",
            endpoint,
            "job",
            "submit",
            path,
            "--approve",
            plan["approval_digest"],
            "--idempotency-key",
            key,
            allow_error=allow_error,
        )
        return response if allow_error else response["data"]

    async def mcp_submit(spec, key):
        from mcp import Client
        from mcp.client.stdio import StdioServerParameters

        params = StdioServerParameters(
            command=str(mcp),
            args=["--profile", "simulation"],
            env={**environment, "HARBOR_CAD_SOCKET": str(endpoint)},
        )
        async with Client(params) as client:
            response = await client.call_tool("filter_plan", {"request_spec": spec})
            if response.is_error:
                raise RuntimeError(response)
            plan = response.structured_content
            response = await client.call_tool(
                "job_submit",
                {
                    "plan": plan["plan"],
                    "approved_digest": plan["approval_digest"],
                    "idempotency_key": key,
                },
            )
            if response.is_error:
                raise RuntimeError(response)
            return plan, response.structured_content

    source = copied / "retained-fields" / time_record["shards"][0]
    source_data = source.read_bytes()
    try:
        with (root / "worker.log").open("xb") as log:
            worker = start(log)
            for interface, field in [("cli", "velocity"), ("mcp", "pressure")]:
                key = f"filter-{interface}"
                spec = request(field)
                if interface == "cli":
                    plan = planned(spec, key)
                    source.write_bytes(b"mutated before filter acknowledgment")
                    assert (
                        submit(plan, "reject-source-mutation", allow_error=True)["ok"]
                        is False
                    )
                    source.write_bytes(source_data)
                    job = submit(plan, key)
                else:
                    plan, job = asyncio.run(mcp_submit(spec, key))
                owned.append(job["unit"])
                assert plan["plan"]["schema_version"] == 4
                assert [s["operation"] for s in plan["plan"]["stages"]] == [
                    "numerical_filter",
                    "bundle",
                ]
                child = (
                    state
                    / "artifacts"
                    / job["id"]
                    / "retained-fields"
                    / time_record["shards"][0]
                )
                assert (
                    child.read_bytes() == source_data
                    and child.stat().st_ino != source.stat().st_ino
                )
                running = wait_job(str(binary), endpoint, job["id"], {"running"})
                properties = subprocess.check_output(
                    [
                        "systemctl",
                        "--user",
                        "show",
                        job["unit"],
                        "--property=InvocationID,MainPID,ControlGroup,MemoryMax,MemorySwapMax,TasksMax,KillMode,CPUQuotaPerSecUSec,NoNewPrivileges",
                    ],
                    text=True,
                )
                controls = dict(
                    line.split("=", 1)
                    for line in properties.splitlines()
                    if "=" in line
                )
                assert controls["InvocationID"] == running["invocation_id"]
                assert controls["MemoryMax"] == str(
                    max(s["ram_bytes"] for s in plan["plan"]["stages"])
                )
                assert (
                    controls["MemorySwapMax"] == "0" and controls["TasksMax"] == "128"
                )
                assert (
                    controls["KillMode"] == "control-group"
                    and controls["NoNewPrivileges"] == "yes"
                )
                assert controls["CPUQuotaPerSecUSec"] == "1s"
                retained = retention_snapshot(state, job, binary)
                assert (
                    retained["intent"]["binding"]["sandbox_policy"]
                    == "harbor-cad-filter-hip-single-kfd-v1"
                )
                reservation = admission_record(state, job)
                assert reservation is not None
                source.write_bytes(b"mutated after filter acknowledgment")
                worker.kill()
                worker.wait(timeout=5)
                worker = start(log)
                assert submit(plan, key)["id"] == job["id"]
                outcome = wait_job(
                    str(binary), endpoint, job["id"], {"succeeded"}, timeout=130
                )
                assert outcome["invocation_id"] == running["invocation_id"]
                source.write_bytes(source_data)
                wait_admission_release(state, job)
                wait_retention_release(state, job)
                destination = root / f"bundle-{interface}"
                command("artifact", "export", "--state", state, job["id"], destination)
                records = verify_manifest(destination)
                assert (
                    not list(destination.glob("*openlb*"))
                    and not (destination / "frame-sequence.json").exists()
                )
                receipt = json.loads(
                    (
                        destination / "stages/filter/numerical_filter-receipt.json"
                    ).read_text()
                )
                description = json.loads(
                    (destination / "stages/filter/field-description.json").read_text()
                )
                assert (
                    receipt["backend"] == "hip"
                    and receipt["software_fallback"] is False
                )
                assert (
                    receipt["science_id"] == snapshot["science_id"]
                    and receipt["execution_id"] == snapshot["execution_id"]
                )
                assert all(
                    description[k] == time_record[k]
                    for k in ("requested_s", "observed_s", "step")
                )
                assert description["filter_execution_id"] == job["plan_digest"]
                assert description["source_science_id"] == snapshot["science_id"]
                assert description["source_execution_id"] == snapshot["execution_id"]
                assert description["field_units"]["gradient"] == (
                    "1/s" if field == "velocity" else "Pa/m"
                )
                _, extent, filtered = read_gradient(
                    destination / "stages/filter/gradient.vti"
                )
                original_field = (
                    "physVelocity" if field == "velocity" else "physPressure"
                )
                _, expected_extent, cpu = read_gradient(
                    args.native_reference / f"openlb-{original_field}-cpu/gradient.vti"
                )
                assert extent == expected_extent
                assert filtered["gradient"][0] == cpu["gradient"][0]
                assert len(filtered["gradient"][1]) == len(cpu["gradient"][1])
                error = max(
                    abs(a - b)
                    for a, b in zip(
                        filtered["gradient"][1], cpu["gradient"][1], strict=True
                    )
                )
                assert error <= 1e-10
                results.append(
                    {
                        "interface": interface,
                        "job": outcome,
                        "bundle_records": len(records),
                        "receipt": receipt,
                        "description": description,
                        "cpu_max_abs_disagreement": error,
                        "active_runtime_retention": retained,
                        "effective_controls": controls,
                        "admission_record": reservation,
                        "restart": "same invocation",
                        "reservation_and_roots_released": True,
                    }
                )
            plan = planned(request(), "forced-death")
            forced = submit(plan, "forced-filter-death")
            owned.append(forced["unit"])
            wait_job(str(binary), endpoint, forced["id"], {"running"})
            forced_retention = retention_snapshot(state, forced, binary)
            worker.kill()
            worker.wait(timeout=5)
            subprocess.run(
                [
                    "systemctl",
                    "--user",
                    "kill",
                    "--signal=KILL",
                    "--kill-whom=all",
                    forced["unit"],
                ],
                check=True,
                timeout=10,
            )
            worker = start(log)
            forced_outcome = wait_job(str(binary), endpoint, forced["id"], {"failed"})
            assert submit(plan, "forced-filter-death")["id"] == forced["id"]
            wait_admission_release(state, forced)
            wait_retention_release(state, forced)
            assert (
                state
                / "artifacts"
                / forced["id"]
                / "retained-fields"
                / time_record["shards"][0]
            ).read_bytes() == source_data
            plan = planned(request(), "cancel")
            job = submit(plan, "cancel-filter")
            owned.append(job["unit"])
            wait_job(str(binary), endpoint, job["id"], {"running"})
            command("--socket", endpoint, "job", "cancel", job["id"])
            wait_job(str(binary), endpoint, job["id"], {"cancelled"})
            wait_admission_release(state, job)
            wait_retention_release(state, job)
            assert source.read_bytes() == source_data
            spec = request()
            spec["filter"]["time_s"] = time_record["requested_s"] / 2 + 0.25
            bad = root / "request-unretained.json"
            bad.write_text(json.dumps(spec))
            assert (
                command("--socket", endpoint, "filter", bad, allow_error=True)["ok"]
                is False
            )
            for record in manifests:
                assert (
                    hashlib.sha256((original / record["path"]).read_bytes()).hexdigest()
                    == record["sha256"]
                )
            report = {
                "schema_version": 1,
                "packages": {
                    "cli": str(binary),
                    "runtime": str(runtime),
                    "mcp": str(mcp),
                },
                "source_job": args.source_job,
                "results": results,
                "source_mutation_before_acknowledgment": "rejected",
                "source_mutation_after_acknowledgment": "verified child bytes through restart/retry",
                "cancellation": "complete tree and admission/runtime-root release; source preserved",
                "forced_death": {
                    "job": forced_outcome,
                    "active_runtime_retention": forced_retention,
                    "retry": "same failed job, no implicit relaunch",
                    "source_preserved": True,
                    "reservation_and_roots_released": True,
                },
                "unretained_time": "rejected",
                "original_source": "registered hashes unchanged",
                "physical_validation": "unqualified",
            }
            (root / "verification.json").write_text(
                json.dumps(report, indent=2, allow_nan=False)
            )
            print(json.dumps(report, indent=2, allow_nan=False))
    finally:
        source.write_bytes(source_data)
        if worker is not None and worker.poll() is None:
            worker.terminate()
            worker.wait(timeout=5)
        for unit in owned:
            subprocess.run(
                ["systemctl", "--user", "stop", unit],
                capture_output=True,
                timeout=10,
                check=False,
            )


if __name__ == "__main__":
    main()
