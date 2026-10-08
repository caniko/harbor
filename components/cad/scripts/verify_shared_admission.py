"""Opt-in packaged same-user admission/lifecycle qualification on synthetic CPU jobs."""

import argparse
import hashlib
import json
import os
import pwd
import socket
import sqlite3
import subprocess
import sys
import time
from pathlib import Path

from verify_systemd import command, wait_job, wait_retention_release


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    binary = args.executable.resolve(strict=True)
    if not binary.is_relative_to("/nix/store") or not binary.is_file():
        raise ValueError("exact packaged runner required")
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    info = command(binary, "doctor")
    case = command(binary, "case", "init")
    case["resolution"] = 1_000_000
    case["applicability"]["numerical_tolerance"] = 0.01
    case_path = root / "case.json"
    case_path.write_text(json.dumps(case))
    planned = command(binary, "case", "plan", case_path)
    plan = planned["plan"]
    plan_path = root / "plan.json"
    plan_path.write_text(json.dumps(plan))
    ram = max(stage["ram_bytes"] for stage in plan["stages"])
    authority = {
        "schema_version": 1,
        "fleetix_revision": info["fleetix_revision"],
        "fleetix_contract_digest": info["fleetix_contract_digest"],
        "max_ram_bytes": ram + 64 * 1024**2,
        "ram_headroom_bytes": 64 * 1024**2,
        "filesystems": [
            {
                "root": str(root.parent),
                "max_bytes": 1024**3,
                "free_headroom_bytes": 128 * 1024**2,
            }
        ],
        "cards": [],
        "routes": [],
        "allowed_devices": [],
        "overrides": [],
        "native_runtimes": [],
        "allowed_input_roots": [str(root), str(root / "lifecycle")],
    }
    authority_path = root / "authority.json"
    authority_path.write_text(json.dumps(authority))
    installed = command(binary, "authority", "install", authority_path)
    ledger = (
        Path(pwd.getpwuid(os.geteuid()).pw_dir)
        / ".local/state/harbor-cad/admission/admission.sqlite3"
    )
    assert Path(installed["admission_root"]) == ledger.parent
    profile = {
        "schema_version": 1,
        "policy": "ci",
        "allowed_input_root": str(root),
        "max_ram_bytes": 512 * 1024**2,
        "max_disk_bytes": 512 * 1024**2,
        "threads": 1,
        "timeout_seconds": 120,
        "native_runtime": None,
        "service_mode": "systemd",
    }
    profile_path = root / "profile.json"
    profile_path.write_text(json.dumps(profile))
    states = [root / "first", root / "second"]
    workers = []
    jobs = []

    def start(state):
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
            ]
        )
        workers.append(process)
        endpoint = state / "worker.sock"
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise RuntimeError("authority worker failed to start")
            if endpoint.exists():
                try:
                    with socket.socket(socket.AF_UNIX) as connection:
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

    def submit(state, key):
        job = command(
            binary,
            "--socket",
            state / "worker.sock",
            "job",
            "submit",
            plan_path,
            "--approve",
            planned["approval_digest"],
            "--idempotency-key",
            key,
        )["data"]
        jobs.append((state, job))
        return job

    def signal(job, value):
        subprocess.run(
            [
                "systemctl",
                "--user",
                "kill",
                "--kill-whom=all",
                f"--signal={value}",
                job["unit"],
            ],
            check=True,
        )

    def reservations():
        with sqlite3.connect(f"file:{ledger}?mode=ro", uri=True) as db:
            return {
                row[0]: json.loads(row[1])
                for row in db.execute("SELECT id,record FROM reservations")
            }

    def queued(state, job):
        observed = command(
            binary, "--socket", state / "worker.sock", "job", "status", job["id"]
        )["data"]
        assert observed["state"] == "queued" and observed["invocation_id"] is None
        loaded = subprocess.check_output(
            [
                "systemctl",
                "--user",
                "show",
                job["unit"],
                "--property=LoadState",
                "--value",
            ],
            text=True,
        ).strip()
        assert loaded == "not-found"
        return observed

    try:
        first_worker = start(states[0])
        start(states[1])
        first = submit(states[0], "survive-worker")
        wait_job(binary, states[0] / "worker.sock", first["id"], {"running"})
        signal(first, "STOP")
        active = reservations()
        assert first["id"] in active and active[first["id"]]["ram"] == ram
        second = submit(states[1], "cross-root-ram")
        time.sleep(0.15)
        before_crash = queued(states[1], second)
        first_worker.kill()
        first_worker.wait(timeout=5)
        after_crash = queued(states[1], second)
        assert first["id"] in reservations()
        signal(first, "CONT")
        second_outcome = wait_job(
            binary, states[1] / "worker.sock", second["id"], {"succeeded"}, timeout=60
        )
        start(states[0])
        first_outcome = wait_job(
            binary, states[0] / "worker.sock", first["id"], {"succeeded"}
        )
        for state, job in [(states[0], first), (states[1], second)]:
            wait_retention_release(state, job)
        occupancy = states[0] / "synthetic-quarantined-occupancy"
        with occupancy.open("wb") as output:
            output.truncate(authority["filesystems"][0]["max_bytes"])
        third = submit(states[1], "cross-root-disk")
        time.sleep(0.15)
        disk_wait = queued(states[1], third)
        occupancy.unlink()
        third_outcome = wait_job(
            binary, states[1] / "worker.sock", third["id"], {"succeeded"}, timeout=60
        )
        wait_retention_release(states[1], third)
        exports = []
        for state, job in jobs:
            target = root / f"bundle-{job['id']}"
            command(binary, "artifact", "export", "--state", state, job["id"], target)
            execution = json.loads((target / "execution.json").read_text())
            authorization = execution["execution_authorization"]
            assert authorization["authority"] == authority
            assert execution["job"]["state"] == "succeeded"
            assert authorization["plan_digest"] == planned["approval_digest"]
            for item in json.loads((target / "manifest.json").read_text()):
                data = (target / item["path"]).read_bytes()
                assert (
                    len(data) == item["bytes"]
                    and hashlib.sha256(data).hexdigest() == item["sha256"]
                )
            exports.append(str(target))
        deadline = time.monotonic() + 10
        while reservations():
            if time.monotonic() >= deadline:
                raise TimeoutError("closed admission reservations not released")
            time.sleep(0.01)
        subprocess.run(
            [
                sys.executable,
                str(Path(__file__).with_name("verify_systemd.py")),
                "--executable",
                str(binary),
                "--output",
                str(root / "lifecycle"),
                "--authority",
                str(authority_path),
            ],
            check=True,
            timeout=120,
        )
        report = {
            "packaged_runner": str(binary),
            "runner_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "authority": authority,
            "installation": installed,
            "cross_root_ram_wait": before_crash,
            "worker_death_preserved_wait": after_crash,
            "cross_root_quarantined_disk_wait": disk_wait,
            "outcomes": [first_outcome, second_outcome, third_outcome],
            "exports": exports,
            "reservations_released": not reservations(),
            "scope": "synthetic CPU references; GPU isolation/VRAM/hardware B1 unqualified",
        }
        assert report["reservations_released"]
        (root / "verification.json").write_text(json.dumps(report, indent=2))
        print(json.dumps(report, indent=2))
    finally:
        for _, job in jobs:
            subprocess.run(
                ["systemctl", "--user", "stop", job["unit"]],
                check=False,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
        for process in workers:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=5)


if __name__ == "__main__":
    main()
