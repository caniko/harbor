"""Opt-in actual FreeCAD/OpenLB partial-field recovery under an owned user service.

Kills only its own service after the initial VTI closes, then verifies recovery
after worker restart and ordinary cancellation. Interrupted solving never counts
as numerical success. Requires an existing user manager; HIP additionally needs
explicit devices and authoritative shared admission.
"""

import argparse
import hashlib
import json
import os
import socket as unix_socket
import subprocess
import time
from pathlib import Path

from verify_native_cpu import verify_manifest
from verify_openlb_cpu import read_vti
from verify_systemd import (
    admission_record,
    retention_snapshot,
    wait_admission_release,
    wait_job,
    wait_retention_release,
)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path, required=True)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--authority", type=Path)
    parser.add_argument("--devices", type=Path)
    args = parser.parse_args()
    if args.devices is not None and args.authority is None:
        raise ValueError("GPU recovery requires explicit authority")
    source = args.executable.resolve(strict=True)
    if not source.is_relative_to("/nix/store") or not source.is_file():
        raise ValueError("exact packaged runner required")
    runtime = args.runtime.resolve(strict=True)
    if not runtime.is_relative_to("/nix/store") or not runtime.is_file():
        raise ValueError("immutable packaged runtime required")
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    binary = source
    expected = hashlib.sha256(source.read_bytes()).hexdigest()
    assert hashlib.sha256(binary.read_bytes()).hexdigest() == expected
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
    state = root / "state"
    socket = state / "worker.sock"
    owned = []
    worker = None
    environment = {
        k: os.environ[k]
        for k in ("HOME", "XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS", "PATH")
        if k in os.environ
    }

    def command(*values):
        return json.loads(
            subprocess.check_output(
                [str(binary), *map(str, values)], env=environment, timeout=30
            )
        )

    def start(log):
        process = subprocess.Popen(
            [str(binary), "worker", "--state", str(state), "--profile", str(profile)]
            + (
                ["--authority", str(args.authority.resolve(strict=True))]
                if args.authority
                else []
            ),
            env=environment,
            stdout=log,
            stderr=subprocess.STDOUT,
        )
        try:
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise RuntimeError("worker startup failed; inspect worker.log")
                if socket.exists():
                    try:
                        with unix_socket.socket(unix_socket.AF_UNIX) as connection:
                            connection.settimeout(1)
                            connection.connect(str(socket))
                            connection.sendall(
                                b'{"protocol_version":1,"request_id":"ready","request":{"operation":"doctor"}}\n'
                            )
                            if json.loads(connection.recv(65536))["ok"]:
                                return process
                    except OSError:
                        pass
                time.sleep(0.01)
            raise TimeoutError("worker did not become ready")
        except BaseException:
            process.terminate()
            process.wait(timeout=5)
            raise

    with (root / "worker.log").open("w") as log:
        try:
            worker = start(log)
            case = command("case", "init")
            case.update(
                length={"value": 0.02, "unit": "m"},
                acceleration={"value": 0.001, "unit": "m/s2"},
                resolution=64,
                max_time_s=20,
            )
            case["applicability"]["formulation"] = "periodic_forced_channel"
            case["applicability"]["numerical_tolerance"] = 0.05
            (root / "case.json").write_text(json.dumps(case))
            approved = (
                command(
                    "case",
                    "plan-b1",
                    root / "case.json",
                    "--devices",
                    args.devices.resolve(strict=True),
                )
                if args.devices
                else command("case", "plan-openlb-reference", root / "case.json")
            )
            (root / "plan.json").write_text(json.dumps(approved["plan"]))
            receipts = []
            for scenario in ("forced-death", "cancel"):
                job = command(
                    "--socket",
                    socket,
                    "job",
                    "submit",
                    root / "plan.json",
                    "--approve",
                    approved["approval_digest"],
                    "--idempotency-key",
                    scenario,
                )["data"]
                owned.append(job["unit"])
                running = wait_job(
                    str(binary), socket, job["id"], {"running"}, timeout=10
                )
                retention = retention_snapshot(state, job, binary)
                reservation = admission_record(state, job) if args.authority else None
                if args.authority:
                    assert reservation is not None
                raw = state / "artifacts" / job["id"] / ".native-incomplete"
                deadline = time.monotonic() + 45
                initial = None
                while time.monotonic() < deadline:
                    for path in raw.glob("tmp/vtkData/data/channel_iT0000000iC*.vti"):
                        if path.is_file() and path.read_bytes().rstrip().endswith(
                            b"</VTKFile>"
                        ):
                            initial = path
                            break
                    if initial:
                        break
                    status = command("--socket", socket, "job", "status", job["id"])[
                        "data"
                    ]
                    if status["state"] != "running":
                        raise RuntimeError(status)
                    time.sleep(0.01)
                if initial is None:
                    raise TimeoutError(
                        "initial actual OpenLB field did not close in time"
                    )
                relative = initial.relative_to(raw)
                initial_hash = hashlib.sha256(initial.read_bytes()).hexdigest()
                owner = json.loads(
                    (state / "artifacts" / job["id"] / "service-owner.json").read_text()
                )
                assert (
                    owner["invocation"] == running["invocation_id"]
                    and owner["unit"] == job["unit"]
                )
                if scenario == "forced-death":
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
                        env=environment,
                        check=True,
                        timeout=15,
                    )
                    socket.unlink(missing_ok=True)
                    worker = start(log)
                    terminal = "failed"
                else:
                    command("--socket", socket, "job", "cancel", job["id"])
                    terminal = "cancelled"
                outcome = wait_job(
                    str(binary), socket, job["id"], {terminal}, timeout=15
                )
                wait_retention_release(state, job)
                if args.authority:
                    wait_admission_release(state, job)
                bundle = root / f"bundle-{scenario}"
                command("artifact", "export", "--state", state, job["id"], bundle)
                records = verify_manifest(bundle)
                retained = bundle / "failed-native" / relative
                assert hashlib.sha256(retained.read_bytes()).hexdigest() == initial_hash
                _, _, shape, fields = read_vti(retained)
                assert (
                    fields["physVelocity"][0] == 3
                    and {"physPressure", "geometry"} <= fields.keys()
                )
                execution = json.loads((bundle / "execution.json").read_text())
                assert (
                    execution["job"]["state"] == terminal
                    and execution["physical_validation"] == "unqualified"
                )
                assert (
                    bundle / "native-runtime.json"
                ).read_bytes() == runtime.read_bytes()
                assert not (bundle / "openlb-receipt.json").exists()
                cad = json.loads(
                    (bundle / "failed-native/cad_fixture-receipt.json").read_text()
                )
                assert (
                    cad["executed"]
                    and cad["backend"] == "cpu"
                    and not cad["software_fallback"]
                )
                active = subprocess.check_output(
                    [
                        "systemctl",
                        "--user",
                        "show",
                        job["unit"],
                        "--property=ActiveState",
                        "--value",
                    ],
                    env=environment,
                    text=True,
                    timeout=15,
                ).strip()
                assert active in {"inactive", "failed"}
                receipts.append(
                    {
                        "test": scenario,
                        "outcome": outcome,
                        "service_owner": owner,
                        "active_runtime_retention": retention,
                        "active_reservation": reservation,
                        "admission_released": args.authority is not None,
                        "terminal_runtime_released": True,
                        "initial_field": str(relative),
                        "initial_field_sha256": initial_hash,
                        "retained_initial_field_unchanged": True,
                        "checksummed_bundle_records": len(records),
                        "native_field_bytes": retained.stat().st_size,
                        "vtk_shape": shape,
                        "vtk_finite_float64_arrays": sorted(fields),
                        "physical_validation": "unqualified",
                    }
                )
            report = {
                "scope": "actual packaged FreeCAD/OpenLB initial VTI retained after owned SIGKILL plus worker restart and ordinary cancellation; interrupted solving not numerical success",
                "backend": json.loads(runtime.read_text())["openlb_backend"],
                "authority": str(args.authority.resolve(strict=True))
                if args.authority
                else None,
                "source_cli": str(source),
                "packaged_cli_sha256": expected,
                "runtime": str(runtime),
                "resolution": 64,
                "results": receipts,
                "physical_validation": "unqualified",
            }
            (root / "verification.json").write_text(json.dumps(report, indent=2) + "\n")
            print(json.dumps(report, indent=2))
        finally:
            if worker is not None:
                worker.terminate()
                worker.wait(timeout=5)
            for unit in owned:
                subprocess.run(
                    ["systemctl", "--user", "stop", unit],
                    env=environment,
                    check=False,
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    timeout=15,
                )


if __name__ == "__main__":
    main()
