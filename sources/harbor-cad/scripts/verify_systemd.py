"""Opt-in real user-manager lifecycle test; operates only on its own job units."""

import argparse
import hashlib
import json
import os
import pwd
import socket as unix_socket
import sqlite3
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


def retention_snapshot(state, job, binary):
    directory = state / "retentions" / job["id"]
    intent = json.loads((directory / "intent.json").read_text())
    ready = json.loads((directory / "ready.json").read_text())
    binding = intent["binding"]
    assert intent["job"] == job["id"] and intent["systemd"] is True
    assert binding["runner"]["path"] == str(Path(binary).resolve(strict=True))
    assert (
        ready["binding_digest"]
        == hashlib.sha256(
            json.dumps(binding, separators=(",", ":"), ensure_ascii=False).encode()
        ).hexdigest()
    )
    assert intent["roots"]
    for index, target in enumerate(intent["roots"]):
        link = directory / f"root-{index:04d}"
        assert link.is_symlink() and str(link.readlink()) == target
        assert Path(target).exists()
    return {"directory": str(directory), "intent": intent, "ready": ready}


def wait_retention_release(state, job, timeout=10):
    deadline = time.monotonic() + timeout
    directory = state / "retentions" / job["id"]
    while directory.exists():
        if time.monotonic() >= deadline:
            raise TimeoutError("terminal job runtime roots were not safely released")
        time.sleep(0.01)


def admission_record(state, job):
    ledger = (
        Path(pwd.getpwuid(os.geteuid()).pw_dir)
        / ".local/state/harbor-cad/admission/admission.sqlite3"
    )
    with sqlite3.connect(f"file:{ledger}?mode=ro", uri=True) as database:
        records = [
            json.loads(row[0])
            for row in database.execute("SELECT record FROM reservations")
        ]
    matches = [r for r in records if r["root"] == str(state) and r["id"] == job["id"]]
    if len(matches) > 1:
        raise ValueError("ambiguous same-user reservation")
    return matches[0] if matches else None


def wait_admission_release(state, job, timeout=10):
    deadline = time.monotonic() + timeout
    while admission_record(state, job) is not None:
        if time.monotonic() >= deadline:
            raise TimeoutError(
                "closed service still owns durable admission reservation"
            )
        time.sleep(0.01)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", required=True)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--authority", type=Path)
    args = parser.parse_args()
    source_binary = Path(args.executable).resolve(strict=True)
    if not source_binary.is_relative_to("/nix/store") or not source_binary.is_file():
        raise ValueError("exact packaged runner required for owned systemd execution")
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    expected_hash = hashlib.sha256(source_binary.read_bytes()).hexdigest()
    binary = str(source_binary)
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
        arguments = [binary, "worker", "--state", str(state), "--profile", str(profile)]
        if args.authority:
            arguments.extend(["--authority", str(args.authority.resolve(strict=True))])
        process = subprocess.Popen(arguments)
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
            retention = retention_snapshot(state, job, binary)
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
            assert observed["MemorySwapMax"] == "0"
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
                assert Path(retention["directory"]).exists()
                socket.unlink(missing_ok=True)
                worker = start_worker()
                outcome = wait_job(binary, socket, job["id"], {"succeeded"})
            elif key == "forced-death":
                worker.kill()
                worker.wait(timeout=5)
                assert Path(retention["directory"]).exists()
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
            wait_retention_release(state, job)
            if args.authority:
                ledger = (
                    Path(pwd.getpwuid(os.geteuid()).pw_dir)
                    / ".local/state/harbor-cad/admission/admission.sqlite3"
                )

                deadline = time.monotonic() + 10
                while True:
                    with sqlite3.connect(
                        f"file:{ledger}?mode=ro", uri=True
                    ) as connection:
                        present = connection.execute(
                            "SELECT count(*) FROM reservations WHERE id=?", (job["id"],)
                        ).fetchone()[0]
                    if not present:
                        break
                    if time.monotonic() >= deadline:
                        raise TimeoutError(
                            "closed service admission reservation retained"
                        )
                    time.sleep(0.01)
            receipts.append(
                {
                    "test": key,
                    "effective_properties": observed,
                    "service_owner": owner,
                    "source_binary": str(source_binary),
                    "packaged_binary_sha256": expected_hash,
                    "active_runtime_retention": retention,
                    "terminal_runtime_released": True,
                    "shared_admission_released": True if args.authority else None,
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
