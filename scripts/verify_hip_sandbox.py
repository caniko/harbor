"""Qualify the single-KFD-GPU sandbox candidate on labelled Float64 channel inputs."""

import argparse
import hashlib
import json
import subprocess
from pathlib import Path

from verify_openlb_cpu import prepare_fixture, verify_fields
from verify_openlb_hip import service_resources


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--probe", type=Path, required=True)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--pci", required=True)
    parser.add_argument("--uuid", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    resources = service_resources()
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    runtime = json.loads(args.runtime.resolve(strict=True).read_text())
    if runtime["openlb_backend"] != "hip":
        raise ValueError("exact HIP runtime required")
    selection = {
        "role": "compute",
        "backend": "hip",
        "pci": args.pci,
        "backend_uuid": args.uuid,
    }
    results = []
    commands = []
    isolation = []
    for resolution in (8, 16):
        directory = prepare_fixture(root, resolution, selection)
        command = [
            str(args.probe.resolve(strict=True)),
            runtime["bwrap"],
            runtime["openlb"],
            args.pci,
            str(directory),
            str(directory / "plan.json"),
            "openlb",
            "--uuid",
            args.uuid,
        ]
        output = subprocess.run(
            command, capture_output=True, text=True, check=False, timeout=180
        )
        (directory / "probe-stdout.json").write_text(output.stdout)
        (directory / "probe-stderr.log").write_text(output.stderr)
        commands.append({"command": command, "exit_code": output.returncode})
        if output.returncode:
            raise RuntimeError(
                f"HIP sandbox probe failed; evidence retained in {directory}"
            )
        probe = json.loads(output.stdout)
        identity = probe["binding"]["identity"]
        result = verify_fields(directory, resolution, selection)
        receipt = result["receipt"]
        assert receipt["architecture"].split(":", 1)[0] == identity["architecture"]
        assert receipt["compiled_architecture"] == identity["architecture"]
        assert receipt["backend_uuid"] == identity["backend_uuid"]
        result["sandbox_probe"] = probe
        results.append(result)
        if resolution == 8:
            # The probe keeps create-new logs. Each negative/inspection invocation
            # gets an independent private work directory, never a previous log.
            for denied in (False, True):
                work = root / ("kfd-only" if denied else "inventory")
                work.mkdir(mode=0o700)
                inspection = list(command)
                inspection[4] = str(work)
                inspection.append("--inventory-only")
                if denied:
                    inspection.append("--deny-render-node")
                observed = subprocess.run(
                    inspection, capture_output=True, text=True, check=False, timeout=30
                )
                (work / "probe-stdout.json").write_text(observed.stdout)
                (work / "probe-stderr.log").write_text(observed.stderr)
                native_log = (work / "openlb-native.log").read_text()
                if denied:
                    if (
                        observed.returncode == 0
                        or native_log.strip() != "no ROCm-capable device is detected"
                    ):
                        raise ValueError(
                            "shared KFD without the selected render node did not reject HIP initialization"
                        )
                else:
                    inventory = json.loads(native_log)
                    if observed.returncode or len(inventory["devices"]) != 1:
                        raise ValueError(
                            "sandbox inventory must resolve exactly one HIP device"
                        )
                    assert inventory["devices"][0]["backend_uuid"] == args.uuid
                    assert inventory["devices"][0]["pci"] == args.pci
                isolation.append(
                    {
                        "command": inspection,
                        "exit_code": observed.returncode,
                        "render_node_denied": denied,
                        "native_log": native_log,
                    }
                )
    if results[1]["relative_l2_error"] >= results[0]["relative_l2_error"]:
        raise ValueError("refinement must reduce analytical error")
    report = {
        "runtime_manifest": str(args.runtime.resolve(strict=True)),
        "files": {
            str(path): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in (
                args.probe.resolve(strict=True),
                args.runtime.resolve(strict=True),
                Path(runtime["openlb"]),
                Path(__file__),
                Path(__file__).with_name("verify_openlb_cpu.py"),
            )
        },
        "commands": commands,
        "results": results,
        "resources": resources,
        "isolation": isolation,
        "scope": "single observed KFD GPU; candidate mount/runtime/numerical compatibility; no second-GPU exclusion claim",
        "worker_lifecycle": "separately qualified",
        "physical_validation": "unqualified",
    }
    (root / "verification.json").write_text(
        json.dumps(report, indent=2, allow_nan=False)
    )
    print(json.dumps(report, indent=2, allow_nan=False))


if __name__ == "__main__":
    main()
