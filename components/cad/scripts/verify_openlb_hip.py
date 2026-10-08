"""Qualify the packaged Float64 HIP channel, separately from worker/KFD isolation.

Uses only labelled procedural geometry. Run inside an independently bounded
user-service fixture. A passing report is numerical/device evidence, not B1 or
production sandbox qualification.
"""

import argparse
import json
import math
import subprocess
import xml.etree.ElementTree as ET
from pathlib import Path

from verify_openlb_cpu import read_vti, run


def service_resources():
    groups = Path("/proc/self/cgroup").read_text().splitlines()
    unified = [line.removeprefix("0::") for line in groups if line.startswith("0::")]
    if len(unified) != 1 or not unified[0].endswith(".service"):
        raise ValueError("numerical probe requires a bounded cgroup v2 service")
    root = Path("/sys/fs/cgroup") / unified[0].lstrip("/")
    values = {
        name: (root / name).read_text().strip()
        for name in (
            "memory.max",
            "memory.swap.max",
            "memory.peak",
            "cpu.max",
            "cpu.stat",
            "pids.max",
        )
    }
    quota, period = values["cpu.max"].split()
    if (
        values["memory.max"] == "max"
        or int(values["memory.max"]) > 2 * 1024**3
        or values["memory.swap.max"] != "0"
        or values["pids.max"] == "max"
        or int(values["pids.max"]) > 128
        or quota == "max"
        or int(quota) > 2 * int(period)
    ):
        raise ValueError(
            "numerical probe requires <=2 GiB RAM, no swap, <=128 tasks and <=2 CPUs"
        )
    return {"cgroup": unified[0], "kernel_controls": values}


def compare(cpu, gpu):
    def snapshots(directory):
        collection = directory / "tmp/vtkData/channel.pvd"
        result = []
        for item in ET.parse(collection).findall("Collection/DataSet"):
            block = collection.parent / item.attrib["file"]
            arrays = [
                read_vti(block.parent / d.attrib["file"])
                for d in ET.parse(block).findall(".//DataSet")
            ]
            result.append((item.attrib["timestep"], arrays))
        return result

    a, b = snapshots(cpu), snapshots(gpu)
    if not a or len(a) != len(b):
        raise ValueError("CPU/HIP retained sequence length mismatch")
    errors = []
    for (cpu_step, cpu_blocks), (gpu_step, gpu_blocks) in zip(a, b, strict=True):
        if not cpu_blocks or cpu_step != gpu_step or len(cpu_blocks) != len(gpu_blocks):
            raise ValueError("CPU/HIP timestep/block mismatch")
        for (ci, ce, cs, cf), (gi, ge, gs, gf) in zip(
            cpu_blocks, gpu_blocks, strict=True
        ):
            if (
                ci.attrib != gi.attrib
                or ce != ge
                or cs != gs
                or cf.keys() != {"physVelocity", "physPressure", "geometry"}
                or cf.keys() != gf.keys()
            ):
                raise ValueError("CPU/HIP topology/coordinates/field mismatch")
            for field, (components, values) in cf.items():
                gc, observed = gf[field]
                if components != gc or len(values) != len(observed):
                    raise ValueError("CPU/HIP association/components mismatch")
                difference = math.sqrt(
                    sum((x - y) ** 2 for x, y in zip(values, observed, strict=True))
                )
                norm = math.sqrt(sum(x * x for x in values))
                error = difference / norm if norm else difference
                # Uniform-channel pressure is near zero: its relative norm can
                # be ill-conditioned. Retain diagnostic differences without
                # inventing a pressure-accuracy acceptance criterion.
                passed = None
                if field == "physVelocity":
                    passed = math.isfinite(error) and error <= 1e-10
                elif field == "geometry":
                    passed = difference == 0
                elif field != "physPressure":
                    raise ValueError(f"unexpected retained field: {field}")
                if passed is False or not math.isfinite(error):
                    raise ValueError(
                        f"CPU/HIP {field} retained L2 disagreement: {error}"
                    )
                errors.append(
                    {
                        "step": cpu_step,
                        "field": field,
                        "absolute_l2_difference": difference,
                        "relative_l2_difference": error,
                        "agreement_passed": passed,
                        "criterion": "relative L2 <= 1e-10"
                        if field == "physVelocity"
                        else "exact material IDs"
                        if field == "geometry"
                        else "diagnostic only; pressure accuracy unqualified",
                    }
                )
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path, required=True)
    parser.add_argument("--cpu-executable", type=Path, required=True)
    parser.add_argument("--pci", required=True)
    parser.add_argument("--uuid", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    service_resources()
    args.output.mkdir(parents=True, exist_ok=False)
    home = args.output / "home"
    home.mkdir()
    env = {"HOME": str(home.resolve()), "LANG": "C.UTF-8", "OMP_NUM_THREADS": "1"}
    executable = str(args.executable.resolve(strict=True))
    inventory = json.loads(
        subprocess.check_output(
            [executable, "--gpu-inventory"], env=env, text=True, timeout=30
        )
    )
    (args.output / "inventory.json").write_text(json.dumps(inventory, indent=2))
    selection = {
        "role": "compute",
        "backend": "hip",
        "pci": args.pci,
        "backend_uuid": args.uuid,
    }
    cpu_root, hip_root = args.output / "cpu", args.output / "hip"
    cpu_root.mkdir()
    hip_root.mkdir()
    results = []
    for resolution in (8, 16):
        cpu = run(
            str(args.cpu_executable.resolve(strict=True)), cpu_root, resolution, env=env
        )
        hip = run(executable, hip_root, resolution, selection, env)
        agreement = compare(
            cpu_root / f"resolution-{resolution}",
            hip_root / f"resolution-{resolution}",
        )
        results.append({"cpu": cpu, "hip": hip, "retained_field_agreement": agreement})
    if results[1]["hip"]["relative_l2_error"] >= results[0]["hip"]["relative_l2_error"]:
        raise ValueError("HIP refinement must reduce analytical velocity error")
    source = hip_root / "resolution-8"
    plan = json.loads((source / "plan.json").read_text())
    rejections = []
    for label, binary, change, extra_env, expected in [
        ("ambiguous_stage", executable, {}, {}, "ambiguous OpenLB stage"),
        (
            "stale_uuid",
            executable,
            {"backend_uuid": "GPU-" + "0" * 32},
            {},
            "HIP UUID/PCI mismatch",
        ),
        (
            "wrong_backend",
            executable,
            {"backend": "vulkan"},
            {},
            "explicit HIP compute",
        ),
        (
            "missing_card",
            executable,
            {"pci": "ffff:ff:1f.7"},
            {},
            "HIP PCI card missing",
        ),
        (
            "architecture_spoof",
            executable,
            {},
            {"HSA_OVERRIDE_GFX_VERSION": "11.0.0"},
            "architecture spoofing",
        ),
        (
            "cpu_fallback",
            str(args.cpu_executable.resolve()),
            {},
            {},
            "CPU driver cannot execute",
        ),
    ]:
        directory = args.output / label
        directory.mkdir()
        changed = json.loads(json.dumps(plan))
        changed["stages"][0]["selection"].update(change)
        if label == "ambiguous_stage":
            changed["stages"].append(changed["stages"][0].copy())
        (directory / "plan.json").write_text(json.dumps(changed))
        output = subprocess.run(
            [binary, "openlb", "plan.json"],
            cwd=directory,
            env={**env, **extra_env},
            text=True,
            capture_output=True,
            timeout=30,
            check=False,
        )
        (directory / "process.log").write_text(output.stdout + output.stderr)
        if (
            output.returncode == 0
            or expected not in output.stderr
            or (directory / "tmp").exists()
        ):
            raise ValueError(f"{label} did not reject before solver output")
        rejections.append(
            {
                "case": label,
                "exit_code": output.returncode,
                "reason": output.stderr.strip(),
            }
        )
    report = {
        "fixture": "synthetic procedural STL; not FreeCAD or worker evidence",
        "results": results,
        "rejections": rejections,
        "kfd_isolation": "unqualified",
        "b1": "unqualified",
        "physical_validation": "unqualified",
        "performance": "not compared",
        "service_resources": service_resources(),
    }
    (args.output / "verification.json").write_text(
        json.dumps(report, indent=2, allow_nan=False)
    )
    print(json.dumps(report, indent=2, allow_nan=False))


if __name__ == "__main__":
    main()
