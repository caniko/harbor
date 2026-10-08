"""Exact-package original-field conduction Stefan/mass/energy/refinement gate."""

import argparse
import json
import os
import subprocess
from itertools import pairwise
from pathlib import Path

from verify_contact_cpu import load
from verify_openlb_hip import service_resources
from verify_wetting_cpu import checksum


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("runtime", "output"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    args = parser.parse_args()
    runtime = args.runtime.resolve(strict=True)
    if not runtime.is_relative_to("/nix/store") or not runtime.is_file():
        raise ValueError("exact immutable packaged freezing runtime required")
    native = json.loads(runtime.read_text())
    if native["schema_version"] != 1 or native["backend"] != "cpu":
        raise ValueError("explicit versioned CPU runtime required")
    closure = Path(native["freezing_closure"]).resolve(strict=True)
    paths = closure.read_text().splitlines()
    if (
        not paths
        or len(paths) != len(set(paths))
        or any(
            Path(p).parent != Path("/nix/store") or not Path(p).exists() for p in paths
        )
        or Path(native["freezing"]).parent.parent.as_posix() not in paths
    ):
        raise ValueError("exact distinct operation closure and adapter required")
    source = Path(__file__).resolve().parents[1]
    bridge = load("freezing_verification", source / "adapters/freezing_reference.py")
    fem = load("freezing_common", source / "adapters/fem_reference.py")
    base = json.loads((source / "examples/freezing-reference.json").read_text())
    before = service_resources()
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)

    def run(name, spec, worker_layout=False):
        inputs, output = root / f"inputs-{name}", root / name
        inputs.mkdir(mode=0o700)
        output.mkdir(mode=0o700)
        if worker_layout:
            (output / "freezing.log").touch(exist_ok=False)
        request = inputs / "request.json"
        request.write_text(json.dumps(spec, allow_nan=False, indent=2))
        command = [
            native["bwrap"],
            "--die-with-parent",
            "--new-session",
            "--unshare-all",
            "--dir",
            "/nix",
            "--dir",
            "/nix/store",
        ]
        for path in paths:
            command.extend(["--ro-bind", path, path])
        command.extend(
            [
                "--ro-bind",
                str(closure),
                "/freezing-runtime-closure.txt",
                "--proc",
                "/proc",
                "--dev",
                "/dev",
                "--tmpfs",
                "/tmp",
                "--tmpfs",
                "/home",
                "--dir",
                "/home/worker",
                "--ro-bind",
                str(inputs),
                "/inputs",
                "--bind",
                str(output),
                "/work",
                "--chdir",
                "/work",
                "--clearenv",
                "--setenv",
                "HOME",
                "/home/worker",
                "--setenv",
                "LC_ALL",
                "C",
                "--setenv",
                "HARBOR_CAD_FREEZING_POLICY",
                bridge.POLICY,
                "--setenv",
                "HARBOR_CAD_HOST_NETNS",
                os.readlink("/proc/self/ns/net"),
                native["freezing"],
                "reference",
                "/inputs/request.json",
            ]
        )
        process = subprocess.run(command, capture_output=True, timeout=200, check=False)
        (root / f"launch-{name}.log").write_bytes(process.stdout + process.stderr)
        return process, output, request, command

    results, rejections = [], []
    for stefan, resolutions in (
        (0.1, (64, 128, 256)),
        (0.2, (128, 256)),
        (0.05, (256,)),
    ):
        for n in resolutions:
            spec = {
                **base,
                "resolution": n,
                "steps": n * n,
                "observation_steps": [0, n * n // 2, n * n],
                "latent_heat_j_kg": 10000 / stefan,
            }
            bridge.validate(spec, fem)
            name = f"stefan{stefan}-n{n}"
            process, output, request, command = run(name, spec, worker_layout=n == 256)
            if process.returncode:
                raise RuntimeError(
                    f"native freezing gate failed; originals retained in {output}"
                )
            receipt = json.loads((output / "freezing-receipt.json").read_text())
            checks, observations, hashes = bridge.verify(spec, receipt, output, fem)
            if (
                receipt["request_sha256"] != checksum(request)
                or receipt["sandbox"]["policy"] != bridge.POLICY
                or len(receipt["sandbox"]["checks"]) != 8
                or not all(receipt["sandbox"]["checks"].values())
            ):
                raise ValueError(
                    "native request bytes and bounded closure-only sandbox canaries required"
                )
            results.append(
                {
                    "stefan_number": stefan,
                    "resolution": n,
                    "request": spec,
                    "receipt": receipt,
                    "independent_checks": checks,
                    "independent_observations": observations,
                    "original_files_sha256": hashes,
                    "argv": command,
                    "exit_code": process.returncode,
                    "initial_layout": "worker_empty_regular_log"
                    if n == 256
                    else "standalone_empty",
                }
            )
    for stefan, n in ((0.05, 64), (0.05, 128), (0.2, 64)):
        spec = {
            **base,
            "resolution": n,
            "steps": n * n,
            "observation_steps": [0, n * n // 2, n * n],
            "latent_heat_j_kg": 10000 / stefan,
        }
        name = f"unresolved-stefan{stefan}-n{n}"
        process, output, _, command = run(name, spec)
        raw = json.loads((output / "freezing-receipt.json").read_text())
        if (
            not process.returncode
            or (output / "native-freezing-receipt.json").exists()
            or "unchanged temperature gate failed" not in process.stderr.decode()
        ):
            raise ValueError(
                "unresolved native temperature must fail without weakened gates"
            )
        try:
            bridge.verify(spec, raw, output, fem)
        except ValueError as error:
            if "unchanged temperature gate failed" not in str(error):
                raise
        else:
            raise ValueError("independent unresolved reference rejection required")
        rejections.append(
            {
                "case": name,
                "exit_code": process.returncode,
                "argv": command,
                "original_fields_retained": True,
                "reason": process.stderr.decode()[-2000:],
            }
        )
    mutations = (
        ("backend", "hip"),
        ("resolution", 2**32 + 64),
        ("steps", True),
        ("observation_steps", [0, 16384.5]),
        ("energy_tolerance", 0.01),
        ("front_tolerance", 0.1),
        ("mass_tolerance", 1e-3),
        ("initial_temperature_k", 275.15),
        ("cold_wall_temperature_k", 283.15),
        ("latent_heat_j_kg", 0),
        ("moisture_risk", {"assessment": "missing", "reason": ""}),
        ("pressure_pa", 1),
    )
    for key, value in mutations:
        spec = {**base, key: value}
        process, output, _, command = run(f"rejected-{key}", spec)
        if (
            not process.returncode
            or (output / "freezing-receipt.json").exists()
            or (output / "process.log").exists()
        ):
            raise ValueError(
                "invalid science/acceptance input must reject before native launch"
            )
        rejections.append(
            {
                "case": key,
                "exit_code": process.returncode,
                "argv": command,
                "before_native_launch": True,
                "reason": process.stderr.decode()[-2000:],
            }
        )
    sequence = [r for r in results if r["stefan_number"] == 0.1]
    refinement = {
        "stefan_number": 0.1,
        "resolutions": [r["resolution"] for r in sequence],
        "retained_physical_times_s": [
            o["physical_time_s"] for o in sequence[0]["independent_observations"]
        ],
        "errors": {
            key: [r["independent_checks"][key]["error"] for r in sequence]
            for key in ("front", "temperature")
        },
        "scope": "equal-physical-time native grid refinement; active control volumes and geometry are explicit per resolution",
        "passed": True,
    }
    if any(
        not all(a > b for a, b in pairwise(errors))
        for errors in refinement["errors"].values()
    ) or any(
        [o["physical_time_s"] for o in r["independent_observations"]]
        != refinement["retained_physical_times_s"]
        for r in sequence
    ):
        raise ValueError(
            "strictly decreasing front and temperature error at equal retained times required"
        )
    report = {
        "schema_version": 1,
        "runtime": str(runtime),
        "runtime_sha256": checksum(runtime),
        "results": results,
        "rejections": rejections,
        "refinement": refinement,
        "service_resources_before": before,
        "service_resources_after": service_resources(),
        "physical_validation": "unqualified",
        "scope": "synthetic equal-phase fixed-volume conduction solidification; exact native adapter/sandbox/original fields; no worker, retained-water transfer or expansion qualification",
    }
    (root / "verification.json").write_text(
        json.dumps(report, allow_nan=False, indent=2)
    )
    print(
        json.dumps(
            {
                "report_sha256": checksum(root / "verification.json"),
                "solves": len(results),
                "rejections": len(rejections),
                "refinement": refinement,
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
