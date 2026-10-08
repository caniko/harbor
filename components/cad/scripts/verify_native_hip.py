"""Packaged authority-bound FreeCAD/HIP/EGL/VAAPI jobs through CLI and official MCP."""

import argparse
import asyncio
import hashlib
import json
import os
import socket as unix_socket
import subprocess
import time
from pathlib import Path

from verify_native_cpu import verify_bundle
from verify_systemd import (
    admission_record,
    retention_snapshot,
    wait_admission_release,
    wait_job,
    wait_retention_release,
)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("executable", "runtime", "mcp", "authority", "devices", "output"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    args = parser.parse_args()
    binary, runtime, mcp = (
        p.resolve(strict=True) for p in (args.executable, args.runtime, args.mcp)
    )
    if any(
        not p.is_relative_to("/nix/store") or not p.is_file()
        for p in (binary, runtime, mcp)
    ):
        raise ValueError("exact packaged runner, runtime and MCP required")
    if json.loads(runtime.read_text())["openlb_backend"] != "hip":
        raise ValueError("HIP runtime required; no solver fallback")
    authority_path = args.authority.resolve(strict=True)
    authority = json.loads(authority_path.read_text())
    devices = json.loads(args.devices.resolve(strict=True).read_text())
    if devices["compute"]["backend"] != "hip":
        raise ValueError("explicit HIP selection required")
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    state, endpoint = root / "state", root / "state/worker.sock"
    profile_path = root / "profile.json"
    profile_path.write_text(
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
    environment = {
        k: os.environ[k]
        for k in ("HOME", "XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS", "PATH")
        if k in os.environ
    }
    owned, worker = [], None

    def command(*values):
        return json.loads(
            subprocess.check_output(
                [str(binary), *map(str, values)], env=environment, timeout=30
            )
        )

    def start(log):
        process = subprocess.Popen(
            [
                str(binary),
                "worker",
                "--state",
                str(state),
                "--profile",
                str(profile_path),
                "--authority",
                str(authority_path),
            ],
            env=environment,
            stdout=log,
            stderr=subprocess.STDOUT,
        )
        try:
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise RuntimeError("worker startup failed; inspect worker.log")
                if endpoint.exists():
                    try:
                        with unix_socket.socket(unix_socket.AF_UNIX) as connection:
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
            raise TimeoutError("worker startup")
        except BaseException:
            process.terminate()
            process.wait(timeout=5)
            raise

    async def submit_mcp(case, planned):
        from mcp import Client
        from mcp.client.stdio import StdioServerParameters

        async with Client(
            StdioServerParameters(
                command=str(mcp),
                args=["--profile", "all"],
                env={**environment, "HARBOR_CAD_SOCKET": str(endpoint)},
            )
        ) as client:
            result = await client.call_tool(
                "case_plan_b1", {"case": case, "selections": devices}
            )
            assert not result.is_error and result.structured_content == planned["plan"]
            result = await client.call_tool(
                "job_submit",
                {
                    "plan": result.structured_content,
                    "approved_digest": planned["approval_digest"],
                    "idempotency_key": "native-hip-mcp",
                },
            )
            assert not result.is_error and result.structured_content["id"]
            return result.structured_content

    with (root / "worker.log").open("w") as log:
        try:
            worker = start(log)
            identity = command("backend", "hip-identity", devices["compute"]["pci"])
            assert identity["backend_uuid"] == devices["compute"]["backend_uuid"]
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
                case["presentation"].update(
                    width=640, height=480, camera=[0.04, 0.02, 0.03]
                )
                case_path, plan_path = (
                    root / f"case-{resolution}.json",
                    root / f"plan-{resolution}.json",
                )
                case_path.write_text(json.dumps(case))
                planned = command(
                    "case",
                    "plan-b1",
                    case_path,
                    "--devices",
                    args.devices.resolve(strict=True),
                )
                plan_path.write_text(json.dumps(planned["plan"]))
                if resolution == 8:
                    job = command(
                        "--socket",
                        endpoint,
                        "job",
                        "submit",
                        plan_path,
                        "--approve",
                        planned["approval_digest"],
                        "--idempotency-key",
                        "native-hip-cli",
                    )["data"]
                else:
                    job = asyncio.run(submit_mcp(case, planned))
                owned.append(job["unit"])
                # Disconnecting the MCP client and killing the scheduling worker
                # must not terminate the durable service or duplicate its job.
                running = wait_job(str(binary), endpoint, job["id"], {"running"})
                active_reservation = admission_record(state, job)
                assert active_reservation is not None
                active_retention = retention_snapshot(state, job, binary)
                worker.kill()
                worker.wait(timeout=5)
                worker = start(log)
                duplicate = command(
                    "--socket",
                    endpoint,
                    "job",
                    "submit",
                    plan_path,
                    "--approve",
                    planned["approval_digest"],
                    "--idempotency-key",
                    "native-hip-cli" if resolution == 8 else "native-hip-mcp",
                )["data"]
                assert duplicate["id"] == job["id"]
                outcome = wait_job(
                    str(binary), endpoint, job["id"], {"succeeded"}, timeout=190
                )
                assert outcome["invocation_id"] == running["invocation_id"]
                wait_retention_release(state, job)
                wait_admission_release(state, job)
                bundle = root / f"bundle-{resolution}"
                command("artifact", "export", "--state", state, job["id"], bundle)
                verified = verify_bundle(bundle, resolution, "hip")
                execution = json.loads((bundle / "execution.json").read_text())
                assert execution["execution_authorization"]["authority"] == authority
                assert (
                    execution["execution_binding"]["sandbox_policy"]
                    == "harbor-cad-native-hip-single-kfd-v1"
                )
                solver = verified["solver"]
                for name in ("pci", "backend_uuid", "architecture"):
                    assert solver[name] == identity[name]
                assert solver["compiled_architecture"] == identity["architecture"]
                assert (
                    solver["compiled_hip_version"]
                    == solver["hip_runtime_version"]
                    == solver["hip_driver_version"]
                )
                assert (
                    solver["gpu_blocks"] == 1
                    and solver["gpu_kernel_completion_verified"] is True
                )
                render = json.loads((bundle / "render-receipt.json").read_text())
                video = json.loads((bundle / "video-receipt.json").read_text())
                snapshot_path = bundle / "retained-fields/snapshot.json"
                snapshot_data = snapshot_path.read_bytes()
                snapshot = json.loads(snapshot_data)
                expected_binding = {
                    "field_snapshot_sha256": hashlib.sha256(snapshot_data).hexdigest(),
                    "field_artifact_id": snapshot["artifact_id"],
                    "science_id": snapshot["science_id"],
                    "execution_id": snapshot["execution_id"],
                }
                assert (
                    snapshot["science_id"]
                    == command("case", "validate", case_path)["science_id"]
                )
                assert snapshot["execution_id"] == planned["approval_digest"]
                for record in snapshot["files"]:
                    payload = (bundle / "retained-fields" / record["path"]).read_bytes()
                    assert (
                        len(payload) == record["bytes"]
                        and hashlib.sha256(payload).hexdigest() == record["sha256"]
                    )
                assert all(
                    render[k] == video[k] == v for k, v in expected_binding.items()
                )
                sequence = (bundle / "frame-sequence.json").read_bytes()
                digest = hashlib.sha256(sequence).hexdigest()
                assert (
                    render["frame_sequence_sha256"]
                    == video["frame_sequence_sha256"]
                    == digest
                )
                frames = json.loads(sequence)["frames"]
                assert frames == render["frames"] and [
                    r["requested_s"] for r in frames
                ] == [0, 10, 20]
                assert video["physical_times_s"] == [0, 10, 20] and video["frames"] == 3
                assert render["observed_camera"] == case["presentation"]["camera"]
                assert render["fixed_range"] == case["presentation"]["range"]
                assert (
                    render["pci"] == devices["render"]["pci"]
                    and video["pci"] == devices["media"]["pci"]
                )
                assert (
                    render["executed"] is True and render["software_fallback"] is False
                )
                assert video["executed"] is True and video["software_fallback"] is False
                assert (
                    video["encoder"] == "h264_vaapi"
                    and video["decode"] == "CPU verification"
                )
                assert (
                    video["metadata"]["width"] == 640
                    and video["metadata"]["height"] == 480
                )
                for frame in frames:
                    payload = (bundle / frame["path"]).read_bytes()
                    assert (
                        len(payload) == frame["bytes"]
                        and hashlib.sha256(payload).hexdigest() == frame["sha256"]
                    )
                assert (bundle / "video.mp4").stat().st_size > 0
                verified.update(
                    interface="CLI" if resolution == 8 else "official MCP stdio client",
                    worker_restart="same service invocation",
                    idempotency="same job",
                    active_reservation=active_reservation,
                    active_runtime_retention=active_retention,
                    reservation_and_runtime_released=True,
                    render=render,
                    video=video,
                )
                results.append(verified)
            assert (
                results[1]["velocity_relative_l2"] < results[0]["velocity_relative_l2"]
            )
            report = {
                "scope": "synthetic periodic channel; durable CAD/HIP/fields/EGL/VAAPI/offline bundle",
                "packages": {
                    "cli": str(binary),
                    "runtime": str(runtime),
                    "mcp": str(mcp),
                },
                "runner_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                "authority": authority,
                "results": results,
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
