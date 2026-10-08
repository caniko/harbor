"""Exact packaged planar contact CLI/MCP, raw outputs and owned lifecycle gate."""

import argparse
import asyncio
import hashlib
import json
import os
import socket
import subprocess
import time
from pathlib import Path

from verify_contact_cpu import load, request
from verify_native_cpu import verify_manifest
from verify_openlb_hip import service_resources
from verify_systemd import (
    admission_record,
    retention_snapshot,
    wait_admission_release,
    wait_job,
    wait_retention_release,
)
from verify_thermal_contact import verify as verify_coupling


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
    parser.add_argument("--thermal-contact", action="store_true")
    parser.add_argument("--thermal-native-reference", type=Path)
    args = parser.parse_args()
    if args.thermal_contact and args.thermal_native_reference is None:
        parser.error("native coupling requires a matching thermal qualification")
    binary, runtime, mcp = (
        p.resolve(strict=True) for p in (args.executable, args.runtime, args.mcp)
    )
    if any(
        not p.is_relative_to("/nix/store") or not p.is_file()
        for p in (binary, runtime, mcp)
    ):
        raise ValueError("exact immutable packaged CLI/runtime/MCP required")
    native = json.loads(runtime.read_text())
    reference_path = args.native_reference / "verification.json"
    reference = json.loads(reference_path.read_text())
    standalone = Path(reference["runtime"])
    if (
        checksum(standalone) != reference["runtime_sha256"]
        or json.loads(standalone.read_text())["contact"] != native["contact"]
        or len(reference["results"]) != 15
        or len(reference["rejections"]) != 8
        or len(reference["refinements"]) != 5
        or not all(v["passed"] for v in reference["refinements"])
    ):
        raise ValueError(
            "matching complete native contact/preload/thermal/spatial gate required"
        )
    if any(
        native.get(k) is not None
        for k in (
            "cad",
            "openlb",
            "render",
            "video",
            "filter",
            "fem",
            "wetting",
            "cad_mesh",
            "fem_imported",
        )
    ):
        raise ValueError("operation-specific CPU contact worker runtime required")
    if args.thermal_contact:
        thermal_reference = json.loads(
            (args.thermal_native_reference / "verification.json").read_text()
        )
        thermal_standalone = Path(thermal_reference["runtime"])
        if (
            checksum(thermal_standalone) != thermal_reference["runtime_sha256"]
            or json.loads(thermal_standalone.read_text())["thermal"]
            != native["thermal"]
            or native["thermal"] != thermal_reference["adapter"]
            or len(thermal_reference["results"]) != 8
            or len(thermal_reference["rejections"]) != 9
            or not thermal_reference["temporal_self_convergence"]["passed"]
            or any(
                not case["receipt"]["numerical_verification"][check]["passed"]
                for case in thermal_reference["results"]
                for check in ("temperature", "energy")
            )
        ):
            raise ValueError(
                "coupling requires the complete matching native thermal/refinement gate"
            )
    elif native.get("thermal") is not None:
        raise ValueError("independent contact runtime required")
    source = Path(__file__).resolve().parents[1]
    bridge = load("contact", source / "adapters/contact_reference.py")
    fem = load("fem", source / "adapters/fem_reference.py")
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
                "timeout_seconds": 600 if args.thermal_contact else 210,
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
    coupling_fixture = (
        json.loads((source / "examples/thermal-contact.json").read_text())
        if args.thermal_contact
        else None
    )
    operation = "thermal-contact" if args.thermal_contact else "contact-reference"

    def command(*argv, allow_error=False):
        p = subprocess.run(
            [str(binary), *map(str, argv)],
            env=environment,
            capture_output=True,
            check=False,
            timeout=30,
        )
        reply = json.loads(p.stdout)
        if p.returncode and not allow_error:
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
                raise RuntimeError("worker failed; original worker.log retained")
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
        raise TimeoutError("owned worker startup")

    def planned(spec, key):
        path = root / ("spec-" + key + ".json")
        path.write_text(json.dumps(spec))
        return command("case", "plan-" + operation, path)

    def submit(plan, key, approval=None, allow_error=False):
        path = root / ("plan-" + key + ".json")
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
                "case_plan_" + operation.replace("-", "_"), {"spec": spec}
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
            for interface, temperatures in (
                ("cli", [273.15, 273.15]),
                ("mcp", [233.15, 233.15]),
            ):
                spec = {**request(8), "final_temperatures_k": temperatures}
                if args.thermal_contact:
                    spec = json.loads(json.dumps(coupling_fixture))
                    if interface == "mcp":
                        for history in spec["thermal"]:
                            history["ambient_history"] = [
                                [0.0, 283.15],
                                [120.0, 283.15],
                            ]
                plan = planned(spec, interface)
                assert (
                    plan["plan"]["schema_version"]
                    == (11 if args.thermal_contact else 10)
                    and plan["plan"][
                        "thermal_contact" if args.thermal_contact else "contact"
                    ]
                    == spec
                    and "case" not in plan["plan"]
                )
                assert plan["plan"]["observation"]["retained_times_s"] == (
                    [10.0, 60.0, 120.0] if args.thermal_contact else []
                )
                assert submit(plan, "wrong-approval", "a" * 64, True)["ok"] is False
                job = (
                    submit(plan, interface)
                    if interface == "cli"
                    else asyncio.run(mcp_submit(spec, plan, interface))
                )
                owned.append(job["unit"])
                running = wait_job(str(binary), endpoint, job["id"], {"running"})
                retained = retention_snapshot(state, job, str(binary))
                assert retained["intent"]["binding"]["sandbox_policy"] == (
                    "harbor-cad-thermal-contact-cpu-v1"
                    if args.thermal_contact
                    else bridge.POLICY
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
                assert submit(plan, interface)["id"] == job["id"]
                outcome = wait_job(
                    str(binary),
                    endpoint,
                    job["id"],
                    {"succeeded"},
                    timeout=650 if args.thermal_contact else 220,
                )
                assert outcome["invocation_id"] == running["invocation_id"]
                wait_admission_release(state, job)
                wait_retention_release(state, job)
                bundle = root / ("bundle-" + interface)
                command("artifact", "export", "--state", state, job["id"], bundle)
                records = verify_manifest(bundle)
                by_path = {v["path"]: v for v in records}
                data = bundle / "stages/contact"
                receipt = json.loads((data / "contact-receipt.json").read_text())
                mesh = json.loads((data / "mesh.json").read_text())
                nodes, cells = (
                    {int(k): v for k, v in mesh[name].items()}
                    for name in ("nodes", "elements")
                )
                fields = fem.read_dat(
                    (data / "reference.dat").read_text(), reaction_forces=True
                )
                mechanical_spec, coupling_checks = (
                    verify_coupling(bundle, spec, job["id"], source)
                    if args.thermal_contact
                    else (spec, None)
                )
                checks = bridge.verify(
                    mechanical_spec, nodes, cells, mesh["boundary_node_sets"], fields
                )
                assert checks == receipt["numerical_verification"]
                assert receipt["request"] == mechanical_spec and receipt[
                    "request_sha256"
                ] == checksum(bundle / "native-contact-request.json")
                assert receipt["sandbox"]["policy"] == bridge.POLICY and all(
                    receipt["sandbox"]["checks"].values()
                )
                assert all(
                    checksum(data / name) == sha
                    for name, sha in receipt["outputs"].items()
                )
                assert (
                    bundle / "native-runtime.json"
                ).read_bytes() == runtime.read_bytes()
                for name in ("fields.json", "reference.dat"):
                    record = by_path["stages/contact/" + name]
                    assert (
                        record["time_s"] is None
                        and record["association"] == "native_node_and_integration_point"
                    )
                    assert (
                        record["units"]
                        == "coordinates m; displacement m; reaction_force N; stress Pa"
                    )
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
                capability = next(
                    c
                    for c in historical["capabilities"]
                    if c["operation"] == "contact_reference"
                )
                assert (
                    capability["runtime_execution"] == "recorded"
                    and capability["numerical_verification"] == "reported_pass"
                )
                assert (
                    capability["formulation"] == mechanical_spec["formulation"]
                    and capability["requested_device"] is None
                )
                if args.thermal_contact:
                    assert len(historical["capabilities"]) == 4
                    assert all(
                        c["runtime_execution"] == "recorded"
                        and c["numerical_verification"] == "reported_pass"
                        for c in historical["capabilities"]
                    )
                    for path in (
                        "stages/projection/projection-receipt.json",
                        "stages/thermal-upper/thermal-fields.json",
                        "stages/thermal-lower/reference.dat",
                    ):
                        original_source = state / "artifacts" / job["id"] / path
                        original_bytes = original_source.read_bytes()
                        original_source.write_bytes(original_bytes + b"changed")
                        try:
                            assert not command(
                                "--socket",
                                endpoint,
                                "qualify",
                                "--job",
                                job["id"],
                                allow_error=True,
                            )["ok"]
                        finally:
                            original_source.write_bytes(original_bytes)
                assert (
                    historical["physical_validation"] == "unqualified"
                    and historical["current_runtime_qualification"] == "not_assessed"
                )
                original = (
                    state / "artifacts" / job["id"] / "stages/contact/reference.dat"
                )
                raw = original.read_bytes()
                original.write_bytes(raw + b"changed")
                try:
                    assert (
                        command(
                            "--socket",
                            endpoint,
                            "qualify",
                            "--job",
                            job["id"],
                            allow_error=True,
                        )["ok"]
                        is False
                    )
                finally:
                    original.write_bytes(raw)
                results.append(
                    {
                        "interface": interface,
                        "job": outcome,
                        "receipt": receipt,
                        "independent_original_field_recheck": checks,
                        "independent_coupling_recheck": coupling_checks,
                        "records": len(records),
                        "service_resources": resources,
                        "active_retention": retained,
                        "admission_record": reservation,
                        "restart": "same invocation",
                        "historical_evidence": historical,
                        "registered_native_mutation_rejected": True,
                        "reservation_and_roots_released": True,
                    }
                )
            for action in ("forced-death", "cancel"):
                plan = planned(
                    coupling_fixture if args.thermal_contact else request(16), action
                )
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
            "binary_sha256": checksum(binary),
            "runtime": str(runtime),
            "runtime_sha256": checksum(runtime),
            "mcp": str(mcp),
            "mcp_sha256": checksum(mcp),
            "native_reference_sha256": checksum(reference_path),
            "results": results,
            "service_resources_before": before,
            "service_resources_after": service_resources(),
            "physical_validation": "unqualified",
            "scope": "synthetic native one-way thermal/capacitance/contact and original-surface moisture"
            if args.thermal_contact
            else "synthetic planar CPU contact/preload/prescribed thermal opening, immutable CLI/MCP jobs, original-field verification, exports and complete owned service lifecycle",
        }
        path = root / "verification.json"
        path.write_text(json.dumps(report, indent=2, allow_nan=False))
        print(
            json.dumps(
                {"report_sha256": checksum(path), "results": len(results)}, indent=2
            )
        )
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
