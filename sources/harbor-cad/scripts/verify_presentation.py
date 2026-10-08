"""Packaged re-rendering of retained authorized science, without solving again."""

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
    for name in ("executable", "runtime", "mcp", "authority", "source-state", "output"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    parser.add_argument("--source-job", required=True)
    parser.add_argument("--independent-video", action="store_true")
    args = parser.parse_args()
    binary, runtime, mcp = (
        p.resolve(strict=True) for p in (args.executable, args.runtime, args.mcp)
    )
    if any(
        not p.is_relative_to("/nix/store") or not p.is_file()
        for p in (binary, runtime, mcp)
    ):
        raise ValueError("exact packaged CLI, runtime and MCP required")
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    state = root / "state"
    state.mkdir(mode=0o700)
    # Fork only the selected completed fixture. Never modify the original state,
    # artifacts, authorization or recorded runtime identity.
    with sqlite3.connect(
        f"file:{args.source_state.resolve() / 'jobs.sqlite3'}?mode=ro", uri=True
    ) as old:
        if old.execute(
            "SELECT count(*) FROM jobs WHERE state IN ('queued','starting','running','cancelling')"
        ).fetchone()[0]:
            raise ValueError("closed source fixture required")
        if old.execute(
            "SELECT state FROM jobs WHERE id=?", (args.source_job,)
        ).fetchone() != ("succeeded",):
            raise ValueError("completed source job required")
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
    if sum(m["bytes"] for m in manifests) > 128 * 1024**2:
        raise ValueError("bounded presentation qualification fixture required")
    original = args.source_state.resolve() / "artifacts" / args.source_job
    copied = state / "artifacts" / args.source_job
    for record in manifests:
        relative = Path(record["path"])
        if relative.is_absolute() or ".." in relative.parts:
            raise ValueError("source artifact path escape")
        source = original / relative
        if any(
            p.is_symlink()
            for p in [source, *source.parents]
            if p.is_relative_to(original)
        ):
            raise ValueError("source artifact symlink")
        data = source.read_bytes()
        if (
            len(data) != record["bytes"]
            or hashlib.sha256(data).hexdigest() != record["sha256"]
        ):
            raise ValueError("source registry mismatch")
        target = copied / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open("xb") as stream:
            stream.write(data)
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
    authority_path = args.authority.resolve(strict=True)
    authority = json.loads(authority_path.read_text())
    devices = {r["role"]: r for r in authority["routes"]}
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
            check=False,
            timeout=30,
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

    def requested(video):
        return {
            "source_job": args.source_job,
            "times_s": [0, 10, 20] if video else [0, 20],
            "presentation": {
                "camera": [0.04, 0.025, 0.035],
                "width": 640,
                "height": 480,
                "field": "velocity",
                "range": [0, 0.0015],
            },
            "render": devices["render"],
            "media": devices["media"] if video else None,
        }

    def planned(spec, name, operation="render"):
        path = root / f"request-{name}.json"
        path.write_text(json.dumps(spec))
        return command("--socket", endpoint, operation, path)["data"]

    def submit(plan, key):
        path = root / f"plan-{key}.json"
        path.write_text(json.dumps(plan["plan"]))
        return command(
            "--socket",
            endpoint,
            "job",
            "submit",
            path,
            "--approve",
            plan["approval_digest"],
            "--idempotency-key",
            key,
        )["data"]

    async def mcp_submit(spec, key="presentation-mcp", tool="render_plan"):
        from mcp import Client
        from mcp.client.stdio import StdioServerParameters

        async with Client(
            StdioServerParameters(
                command=str(mcp),
                args=["--profile", "results"],
                env={**environment, "HARBOR_CAD_SOCKET": str(endpoint)},
            )
        ) as client:
            prepared = await client.call_tool(tool, {"request_spec": spec})
            assert not prepared.is_error, prepared
            plan = prepared.structured_content
            response = await client.call_tool(
                "presentation_submit",
                {
                    "plan": plan["plan"],
                    "approved_digest": plan["approval_digest"],
                    "idempotency_key": key,
                },
            )
            assert not response.is_error, response
            return plan, response.structured_content

    snapshot_bytes = (copied / "retained-fields/snapshot.json").read_bytes()
    snapshot = json.loads(snapshot_bytes)
    with (root / "worker.log").open("w") as log:
        try:
            worker = start(log)
            bad = requested(False)
            bad["times_s"] = [5]
            path = root / "unretained.json"
            path.write_text(json.dumps(bad))
            assert (
                command("--socket", endpoint, "render", path, allow_error=True)["ok"]
                is False
            )
            for interface in ("cli", "mcp"):
                spec = requested(interface == "mcp")
                plan, job = (
                    (planned(spec, interface), None)
                    if interface == "cli"
                    else asyncio.run(mcp_submit(spec))
                )
                if job is None:
                    job = submit(plan, "presentation-cli")
                owned.append(job["unit"])
                # All source bytes are already committed before acknowledgment.
                child = state / "artifacts" / job["id"] / "retained-fields"
                assert (child / "snapshot.json").read_bytes() == snapshot_bytes
                for record in snapshot["files"]:
                    parent_file, child_file = (
                        copied / "retained-fields" / record["path"],
                        child / record["path"],
                    )
                    assert parent_file.read_bytes() == child_file.read_bytes()
                    assert parent_file.stat().st_ino != child_file.stat().st_ino
                # Controlled fork only: acknowledged jobs must use their own
                # committed inodes, including through worker restart/retry.
                if interface == "cli":
                    (copied / "retained-fields/snapshot.json").write_bytes(
                        b"changed after acknowledgment"
                    )
                running = wait_job(str(binary), endpoint, job["id"], {"running"})
                active_retention = retention_snapshot(state, job, binary)
                assert admission_record(state, job) is not None
                worker.kill()
                worker.wait(timeout=5)
                worker = start(log)
                assert submit(plan, f"presentation-{interface}")["id"] == job["id"]
                outcome = wait_job(
                    str(binary), endpoint, job["id"], {"succeeded"}, timeout=130
                )
                assert outcome["invocation_id"] == running["invocation_id"]
                if interface == "cli":
                    (copied / "retained-fields/snapshot.json").write_bytes(
                        snapshot_bytes
                    )
                wait_retention_release(state, job)
                wait_admission_release(state, job)
                bundle = root / f"bundle-{interface}"
                command("artifact", "export", "--state", state, job["id"], bundle)
                records = verify_manifest(bundle)
                execution = json.loads((bundle / "execution.json").read_text())
                assert (
                    execution["execution_binding"]["sandbox_policy"]
                    == "harbor-cad-presentation-v1"
                )
                render = json.loads((bundle / "render-receipt.json").read_text())
                assert render["presentation_execution_id"] == plan["approval_digest"]
                assert render["execution_id"] == snapshot["execution_id"]
                assert render["observed_camera"] == spec["presentation"]["camera"]
                assert render["physical_times_s"] == spec["times_s"]
                events = command(
                    "--socket", endpoint, "job", "logs", job["id"], "--limit", 100
                )["data"]
                assert {
                    e["message"] for e in events if e["kind"] == "stage_started"
                } == (
                    {"render", "video", "bundle"}
                    if interface == "mcp"
                    else {"render", "bundle"}
                )
                if interface == "mcp":
                    video = json.loads((bundle / "video-receipt.json").read_text())
                    assert video["presentation_execution_id"] == plan["approval_digest"]
                    assert (
                        video["frames"] == 3 and video["decode"] == "CPU verification"
                    )
                results.append(
                    {
                        "interface": interface,
                        "job": outcome,
                        "bundle_records": len(records),
                        "render": render,
                        "active_runtime_retention": active_retention,
                        "restart": "same invocation",
                        "reservation_and_roots_released": True,
                    }
                )
            independent = []
            if args.independent_video:
                source_render = results[0]["job"]["id"]
                for interface in ("cli", "mcp"):
                    spec = {"source_job": source_render, "media": devices["media"]}
                    key = f"independent-video-{interface}"
                    if interface == "cli":
                        plan = planned(spec, key, "video")
                        parent = state / "artifacts" / source_render
                        original_frame = (parent / "frame0000.png").read_bytes()
                        (parent / "frame0000.png").write_bytes(
                            b"changed before video acknowledgment"
                        )
                        rejected_plan = root / "plan-reject-frame-mutation.json"
                        rejected_plan.write_text(json.dumps(plan["plan"]))
                        assert (
                            command(
                                "--socket",
                                endpoint,
                                "job",
                                "submit",
                                rejected_plan,
                                "--approve",
                                plan["approval_digest"],
                                "--idempotency-key",
                                "reject-frame-mutation",
                                allow_error=True,
                            )["ok"]
                            is False
                        )
                        (parent / "frame0000.png").write_bytes(original_frame)
                        job = submit(plan, key)
                    else:
                        plan, job = asyncio.run(mcp_submit(spec, key, "video_plan"))
                    owned.append(job["unit"])
                    frames = state / "artifacts" / job["id"] / "retained-frames"
                    parent = state / "artifacts" / source_render
                    assert (frames / "frame-sequence.json").read_bytes() == (
                        parent / "frame-sequence.json"
                    ).read_bytes()
                    assert (frames / "frame0000.png").stat().st_ino != (
                        parent / "frame0000.png"
                    ).stat().st_ino
                    sequence = json.loads((frames / "frame-sequence.json").read_text())
                    for row in sequence["frames"]:
                        assert (frames / row["path"]).read_bytes() == (
                            parent / row["path"]
                        ).read_bytes()
                        assert (frames / row["path"]).stat().st_ino != (
                            parent / row["path"]
                        ).stat().st_ino
                    original_frame = (parent / "frame0000.png").read_bytes()
                    (parent / "frame0000.png").write_bytes(
                        b"changed after video acknowledgment"
                    )
                    running = wait_job(str(binary), endpoint, job["id"], {"running"})
                    active_retention = retention_snapshot(state, job, binary)
                    assert admission_record(state, job) is not None
                    worker.kill()
                    worker.wait(timeout=5)
                    worker = start(log)
                    assert submit(plan, key)["id"] == job["id"]
                    outcome = wait_job(
                        str(binary), endpoint, job["id"], {"succeeded"}, timeout=130
                    )
                    assert outcome["invocation_id"] == running["invocation_id"]
                    (parent / "frame0000.png").write_bytes(original_frame)
                    assert submit(plan, key)["id"] == job["id"]
                    wait_retention_release(state, job)
                    wait_admission_release(state, job)
                    bundle = root / f"bundle-{key}"
                    command("artifact", "export", "--state", state, job["id"], bundle)
                    records = verify_manifest(bundle)
                    receipt = json.loads((bundle / "video-receipt.json").read_text())
                    assert receipt["frames"] == 2 and receipt["physical_times_s"] == [
                        0,
                        20,
                    ]
                    assert (
                        receipt["source_render_execution_id"]
                        == plan["plan"]["frames"]["plan_digest"]
                    )
                    assert (
                        receipt["presentation_execution_id"] == plan["approval_digest"]
                    )
                    events = command(
                        "--socket", endpoint, "job", "logs", job["id"], "--limit", 100
                    )["data"]
                    assert {
                        e["message"] for e in events if e["kind"] == "stage_started"
                    } == {"video", "bundle"}
                    independent.append(
                        {
                            "interface": interface,
                            "job": outcome,
                            "bundle_records": len(records),
                            "video": receipt,
                            "active_runtime_retention": active_retention,
                            "frame_mutation_after_acknowledgment": "committed child inodes used through restart/retry",
                            "restart": "same invocation",
                            "reservation_and_roots_released": True,
                        }
                    )
            immutable = planned(requested(False), "mutation")
            (copied / "retained-fields/snapshot.json").write_bytes(
                b"changed before acknowledgment"
            )
            path = root / "plan-mutation.json"
            path.write_text(json.dumps(immutable["plan"]))
            assert (
                command(
                    "--socket",
                    endpoint,
                    "job",
                    "submit",
                    path,
                    "--approve",
                    immutable["approval_digest"],
                    "--idempotency-key",
                    "reject-source-mutation",
                    allow_error=True,
                )["ok"]
                is False
            )
            (copied / "retained-fields/snapshot.json").write_bytes(snapshot_bytes)
            plan = planned(requested(True), "cancel")
            job = submit(plan, "presentation-cancel")
            owned.append(job["unit"])
            wait_job(str(binary), endpoint, job["id"], {"running"})
            command("--socket", endpoint, "job", "cancel", job["id"])
            wait_job(str(binary), endpoint, job["id"], {"cancelled"})
            wait_retention_release(state, job)
            wait_admission_release(state, job)
            cancelled = root / "bundle-cancel"
            command("artifact", "export", "--state", state, job["id"], cancelled)
            verify_manifest(cancelled)
            assert (
                cancelled / "retained-fields/snapshot.json"
            ).read_bytes() == snapshot_bytes
            for record in manifests:
                assert (
                    hashlib.sha256((original / record["path"]).read_bytes()).hexdigest()
                    == record["sha256"]
                )
            report = {
                "scope": "standalone retained-field EGL/VAAPI presentation; no CAD or solver stages",
                "source_job": args.source_job,
                "packages": {
                    "cli": str(binary),
                    "runtime": str(runtime),
                    "mcp": str(mcp),
                },
                "results": results,
                "independent_video": independent,
                "frame_mutation_before_acknowledgment": "rejected"
                if args.independent_video
                else "not tested",
                "source_mutation_before_acknowledgment": "rejected",
                "source_mutation_after_acknowledgment": "original committed child inodes used through restart/retry",
                "cancellation": "complete tree; source bytes preserved; roots/reservation released",
                "unretained_time": "rejected",
                "original_source": "all registered hashes unchanged",
                "physical_validation": "unqualified",
            }
            (root / "verification.json").write_text(
                json.dumps(report, indent=2, allow_nan=False)
            )
            print(json.dumps(report, indent=2, allow_nan=False))
        finally:
            if worker is not None and worker.poll() is None:
                worker.terminate()
                worker.wait(timeout=5)
            for unit in owned:
                subprocess.run(
                    ["systemctl", "--user", "stop", unit],
                    env=environment,
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    check=False,
                    timeout=15,
                )


if __name__ == "__main__":
    main()
