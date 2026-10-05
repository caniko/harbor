"""Opt-in real user-manager lifecycle test; operates only on its own job units."""

import argparse
import hashlib
import json
import shutil
import socket as unix_socket
import subprocess
import time
from pathlib import Path


def command(binary, *args):
    return json.loads(subprocess.check_output([binary, *map(str, args)]))


def wait_job(binary, socket, job, states, timeout=30):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = command(binary, "--socket", socket, "job", "status", job)["data"]
        if result["state"] in states and (
            result["state"] != "running" or result["invocation_id"]
        ):
            return result
        if result["state"] in {"failed", "interrupted", "cancelled"}:
            raise RuntimeError(result)
        time.sleep(0.01)
    raise TimeoutError(f"job did not reach {states}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    source_binary = Path(args.executable).resolve()
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    # Cargo may atomically replace a development executable while a worker is
    # alive. Freeze one verified inode so current_exe() never names a deleted
    # image during this opt-in qualification.
    binary_path = root / "harbor-cad-tested"
    expected_hash = hashlib.sha256(source_binary.read_bytes()).hexdigest()
    shutil.copyfile(source_binary, binary_path)
    binary_path.chmod(0o500)
    assert hashlib.sha256(binary_path.read_bytes()).hexdigest() == expected_hash
    binary = str(binary_path)
    state = root / "state"
    socket = state / "worker.sock"
    profile = root / "profile.json"
    profile.write_text(
        json.dumps(
            {
                "schema_version": 1,
                "policy": "ci",
                "allowed_input_root": str(root),
                "max_ram_bytes": 512 * 1024**2,
                "max_disk_bytes": 256 * 1024**2,
                "threads": 1,
                "timeout_seconds": 60,
                "native_runtime": None,
                "service_mode": "systemd",
            }
        )
    )
    worker = None
    owned = []

    def start_worker():
        process = subprocess.Popen(
            [binary, "worker", "--state", str(state), "--profile", str(profile)]
        )
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise RuntimeError("worker failed to start")
            if socket.exists():
                try:
                    with unix_socket.socket(unix_socket.AF_UNIX) as connection:
                        connection.connect(str(socket))
                        connection.sendall(
                            b'{"protocol_version":1,"request_id":"ready","request":{"operation":"doctor"}}\n'
                        )
                        if json.loads(connection.recv(65536))["ok"]:
                            return process
                except OSError:
                    pass
            time.sleep(0.01)
        raise TimeoutError("worker socket")

    try:
        worker = start_worker()
        case = command(binary, "case", "init")
        case["resolution"] = 1_000_000
        case["applicability"]["numerical_tolerance"] = 0.01
        case_path = root / "case.json"
        case_path.write_text(json.dumps(case))
        standard = command(binary, "case", "plan", case_path)
        plan = standard["plan"]
        plan_path = root / "plan.json"
        plan_path.write_text(json.dumps(plan))
        receipts = []
        for key in ("restart", "forced-death", "cancel"):
            job = command(
                binary,
                "--socket",
                socket,
                "job",
                "submit",
                plan_path,
                "--approve",
                standard["approval_digest"],
                "--idempotency-key",
                key,
            )["data"]
            owned.append(job["unit"])
            running = wait_job(binary, socket, job["id"], {"running"})
            properties = subprocess.check_output(
                [
                    "systemctl",
                    "--user",
                    "show",
                    job["unit"],
                    "--property=InvocationID,MainPID,ControlGroup,MemoryMax,TasksMax,KillMode,CPUQuotaPerSecUSec,NoNewPrivileges",
                ],
                text=True,
            )
            observed = dict(
                line.split("=", 1) for line in properties.splitlines() if "=" in line
            )
            assert observed["InvocationID"] == running["invocation_id"]
            assert observed["MemoryMax"] == str(
                max(s["ram_bytes"] for s in plan["stages"])
            )
            assert observed["KillMode"] == "control-group"
            assert observed["TasksMax"] == "128"
            assert observed["NoNewPrivileges"] == "yes"
            owner = json.loads(
                (state / "artifacts" / job["id"] / "service-owner.json").read_text()
            )
            assert owner["unit"] == job["unit"]
            assert owner["invocation"] == observed["InvocationID"]
            assert owner["main_pid"] == int(observed["MainPID"])
            assert owner["control_group"] == observed["ControlGroup"]
            if key != "restart":
                # Exercise durable recovery independently of CAD/solver builds.
                # These are explicitly simulated opaque interrupted-adapter bytes.
                raw = state / "artifacts" / job["id"] / ".native-incomplete"
                raw.mkdir(mode=0o700)
                (raw / "solver.partial").write_bytes(
                    b"synthetic recovery fixture; not solver evidence"
                )
            if key == "restart":
                worker.kill()
                worker.wait(timeout=5)
                socket.unlink(missing_ok=True)
                worker = start_worker()
                outcome = wait_job(binary, socket, job["id"], {"succeeded"})
            elif key == "forced-death":
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
                socket.unlink(missing_ok=True)
                worker = start_worker()
                outcome = wait_job(binary, socket, job["id"], {"failed"})
            else:
                command(binary, "--socket", socket, "job", "cancel", job["id"])
                outcome = wait_job(binary, socket, job["id"], {"cancelled"})
                active = subprocess.check_output(
                    [
                        "systemctl",
                        "--user",
                        "show",
                        job["unit"],
                        "--property=ActiveState",
                        "--value",
                    ],
                    text=True,
                ).strip()
                assert active in {"inactive", "failed"}
            if key != "restart":
                bundle = root / f"bundle-{key}"
                command(
                    binary, "artifact", "export", "--state", state, job["id"], bundle
                )
                assert (
                    bundle / "failed-native/solver.partial"
                ).read_bytes() == b"synthetic recovery fixture; not solver evidence"
                failure = json.loads((bundle / "native-failure.json").read_text())
                assert failure["execution"] == outcome["state"]
                assert failure["physical_validation"] == "unqualified"
            receipts.append(
                {
                    "test": key,
                    "effective_properties": observed,
                    "service_owner": owner,
                    "source_binary": str(source_binary),
                    "frozen_binary_sha256": expected_hash,
                    "raw_fixture_scope": "synthetic interrupted-adapter bytes; not native solver evidence"
                    if key != "restart"
                    else None,
                    "outcome": outcome,
                }
            )
        (root / "verification.json").write_text(json.dumps(receipts, indent=2))
        print(json.dumps(receipts, indent=2))
    finally:
        if worker is not None:
            worker.terminate()
            worker.wait(timeout=5)
        for unit in owned:
            subprocess.run(["systemctl", "--user", "stop", unit], check=False)
            subprocess.run(
                ["systemctl", "--user", "reset-failed", unit],
                check=False,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )


if __name__ == "__main__":
    main()
