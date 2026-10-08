"""Qualify production CAD inspection through the packaged worker with live canaries.

The source must be the controlled synthetic FCStd fixture with a fluid solid.
All credential/session probes use synthetic operator-owned canaries only.
"""

import argparse
import hashlib
import json
import os
import shutil
import socket
import subprocess
import time
from pathlib import Path

from verify_native_cpu import verify_manifest
from verify_systemd import retention_snapshot, wait_job, wait_retention_release


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for option in ["executable", "runtime", "source", "output"]:
        parser.add_argument(f"--{option}", type=Path, required=True)
    args = parser.parse_args()
    binary, runtime, source = (
        p.resolve(strict=True) for p in [args.executable, args.runtime, args.source]
    )
    if any(
        not p.is_relative_to("/nix/store") or not p.is_file() for p in [binary, runtime]
    ):
        raise ValueError("exact packaged runner/runtime required")
    root = args.output.resolve()
    root.mkdir(parents=True, mode=0o700, exist_ok=False)
    original = source.read_bytes()
    copied = root / "source.FCStd"
    shutil.copyfile(source, copied)
    canaries = root / "canaries"
    canaries.mkdir(mode=0o700)
    marker = canaries / "credential.canary"
    marker.write_bytes(b"synthetic canary; no real credential")
    state, profile_path = root / "state", root / "profile.json"
    worker_socket = state / "worker.sock"
    profile = {
        "schema_version": 1,
        "policy": "production",
        "allowed_input_root": str(root),
        "max_ram_bytes": 2 * 1024**3,
        "max_disk_bytes": 128 * 1024**2,
        "threads": 1,
        "timeout_seconds": 60,
        "native_runtime": str(runtime),
        "service_mode": "systemd",
    }
    profile_path.write_text(json.dumps(profile))
    environment = {
        k: os.environ[k]
        for k in ["HOME", "PATH", "XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS"]
        if k in os.environ
    }
    owned, worker = [], None

    def command(*values):
        return json.loads(
            subprocess.check_output(
                [str(binary), *map(str, values)], env=environment, timeout=30
            )
        )

    with (
        socket.socket(socket.AF_INET) as network,
        socket.socket(socket.AF_UNIX) as session,
        (root / "worker.log").open("w") as log,
    ):
        network.bind(("127.0.0.1", 0))
        network.listen(2)
        session.bind(str(canaries / "session.sock"))
        session.listen(2)
        worker_environment = {
            **environment,
            "HARBOR_CAD_IMPORT_PROBE_ROOT": str(canaries),
            "HARBOR_CAD_IMPORT_PROBE_PORT": str(network.getsockname()[1]),
            "HARBOR_CAD_CREDENTIAL_SENTINEL": "synthetic-environment-canary",
        }
        try:
            worker = subprocess.Popen(
                [
                    str(binary),
                    "worker",
                    "--state",
                    str(state),
                    "--profile",
                    str(profile_path),
                ],
                env=worker_environment,
                stdout=log,
                stderr=subprocess.STDOUT,
            )
            deadline = time.monotonic() + 10
            while not worker_socket.exists():
                if worker.poll() is not None or time.monotonic() >= deadline:
                    raise RuntimeError(
                        "production worker startup failed; inspect worker.log"
                    )
                time.sleep(0.01)
            results = []
            for scenario, regions in [
                ("valid", ["fluid"]),
                ("missing-region", ["wall"]),
            ]:
                case = command("case", "init")
                case["geometry"] = {
                    "source": copied.name,
                    "sha256": hashlib.sha256(original).hexdigest(),
                    "synthetic": True,
                }
                case["regions"] = regions
                case_path, plan_path = (
                    root / f"{scenario}-case.json",
                    root / f"{scenario}-plan.json",
                )
                case_path.write_text(json.dumps(case))
                approved = command(
                    "case",
                    "plan-cad-inspection",
                    case_path,
                    "--policy",
                    "production",
                    "--max-artifact-bytes",
                    str(16 * 1024**2),
                )
                plan_path.write_text(json.dumps(approved["plan"]))
                job = command(
                    "--socket",
                    worker_socket,
                    "job",
                    "submit",
                    plan_path,
                    "--approve",
                    approved["approval_digest"],
                    "--idempotency-key",
                    scenario,
                )["data"]
                owned.append(job["unit"])
                running = wait_job(
                    str(binary), worker_socket, job["id"], {"running"}, timeout=10
                )
                retention = retention_snapshot(state, job, binary)
                properties = dict(
                    line.split("=", 1)
                    for line in subprocess.check_output(
                        [
                            "systemctl",
                            "--user",
                            "show",
                            job["unit"],
                            "--property=MemoryMax,TasksMax,KillMode,CPUQuotaPerSecUSec,NoNewPrivileges,RuntimeMaxUSec,InvocationID,ControlGroup",
                        ],
                        text=True,
                        env=environment,
                        timeout=15,
                    ).splitlines()
                    if "=" in line
                )
                assert properties["InvocationID"] == running["invocation_id"]
                assert int(properties["MemoryMax"]) == max(
                    stage["ram_bytes"] for stage in approved["plan"]["stages"]
                )
                assert (
                    properties["TasksMax"] == "128"
                    and properties["KillMode"] == "control-group"
                )
                assert (
                    properties["NoNewPrivileges"] == "yes"
                    and properties["CPUQuotaPerSecUSec"] == "1s"
                )
                assert properties["RuntimeMaxUSec"] == "1min"
                outcome = wait_job(
                    str(binary),
                    worker_socket,
                    job["id"],
                    {"succeeded" if scenario == "valid" else "failed"},
                    timeout=70,
                )
                wait_retention_release(state, job)
                bundle = root / f"bundle-{scenario}"
                command("artifact", "export", "--state", state, job["id"], bundle)
                records = verify_manifest(bundle)
                output = bundle if scenario == "valid" else bundle / "failed-native"
                isolation = json.loads((output / "import-isolation.json").read_text())
                probes = isolation["qualification_probes"]
                assert (
                    isolation["policy"] == "harbor-cad-importer-v1"
                    and isolation["input_read_only"]
                )
                assert isolation["net_namespace"] != isolation["host_net_namespace"]
                assert (
                    isolation["gpu_devices_absent"]
                    and isolation["package_mounts_read_only"]
                )
                assert (
                    isolation["file_size_limit_bytes"]
                    == [profile["max_disk_bytes"]] * 2
                )
                assert all(
                    probes[field]
                    for field in [
                        "credential_canary_hidden",
                        "credential_environment_hidden",
                        "session_socket_denied",
                        "host_loopback_denied",
                    ]
                )
                assert probes["descendant_namespace_pid"] > 0
                assert (bundle / "input.FCStd").read_bytes() == original
                group = Path("/sys/fs/cgroup") / properties["ControlGroup"].lstrip("/")
                assert (
                    not group.exists()
                    or "populated 0"
                    in (group / "cgroup.events").read_text().splitlines()
                )
                if scenario == "missing-region":
                    assert not (output / "cad_inspect-receipt.json").exists()
                    assert (
                        "missing or ambiguous named regions"
                        in json.loads((output / "import-error.json").read_text())[
                            "error"
                        ]
                    )
                else:
                    receipt = json.loads(
                        (output / "cad_inspect-receipt.json").read_text()
                    )
                    assert (
                        receipt["executed"]
                        and receipt["import_policy"] == isolation["policy"]
                    )
                results.append(
                    {
                        "scenario": scenario,
                        "job": outcome,
                        "service_properties": properties,
                        "active_runtime_retention": retention,
                        "terminal_runtime_released": True,
                        "isolation": isolation,
                        "checksummed_bundle_records": len(records),
                        "complete_descendant_tree_closed": True,
                    }
                )
            assert marker.read_bytes() == b"synthetic canary; no real credential"
            assert source.read_bytes() == original and copied.read_bytes() == original
            report = {
                "scope": "production packaged worker, closure-only importer mounts, live host canaries, resource controls and descendant cleanup on success/rejection",
                "source": str(source),
                "source_sha256": hashlib.sha256(original).hexdigest(),
                "runner": str(binary),
                "runtime": str(runtime),
                "results": results,
                "production_importer_policy": "passed for controlled fixture and tested abuse cases",
                "physical_validation": "unqualified",
            }
            (root / "verification.json").write_text(json.dumps(report, indent=2))
            print(json.dumps(report, indent=2))
        finally:
            if worker is not None:
                worker.terminate()
                worker.wait(timeout=5)
            for unit in owned:
                subprocess.run(
                    ["systemctl", "--user", "stop", unit],
                    env=environment,
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    timeout=15,
                    check=False,
                )


if __name__ == "__main__":
    main()
