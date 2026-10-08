"""Exact packaged cooling, source/boundary conservation and complete-history gates."""

import argparse
import copy
import hashlib
import importlib.util
import json
import math
import os
import subprocess
from pathlib import Path

from verify_openlb_hip import service_resources
from verify_retained_cooling_history import history, refinements, verifier


def checksum(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def uniform_analytic(module, spec, original, work):
    normalized = module.normalize(spec, original)
    if any(p[2] != 1.0 for p in normalized["parents"].values()):
        raise ValueError(
            "independently manufactured uniform full-water reference required"
        )
    thermal = spec["thermal"]
    alpha = thermal["conductivity_w_m_k"] / (
        thermal["density_kg_m3"] * thermal["specific_heat_j_kg_k"]
    )
    lo, hi = 0.0, 2.0
    for _ in range(100):
        value = (lo + hi) / 2
        if (
            math.sqrt(math.pi) * value * math.exp(value * value) * math.erf(value)
            < thermal["stefan_number_at_full_water_fraction"]
        ):
            lo = value
        else:
            hi = value
    similarity = (lo + hi) / 2
    observations = []
    spacing = normalized["spacing_m"]
    nx = spec["source_shape"][0] * spec["spatial_refinement"]
    height = (spec["source_shape"][1] - 2) * spec["spacing_m"]
    for step in spec["observation_steps"][1:]:
        diffusion_length = math.sqrt(alpha * step * normalized["physical_step_s"])
        front = 2 * similarity * diffusion_length
        errors, solid = [], []
        for row in module.rows(
            module.read_regular(work / f"cooling-{step}.csv", 64 * 1024**2),
            module.COLUMNS,
        ):
            distance = (
                float(row["y_m"])
                - spec["destination_origin_m"][1]
                - 0.5 * spec["spacing_m"]
            )
            expected = (
                math.erf(distance / (2 * diffusion_length)) / math.erf(similarity)
                if distance < front
                else 1.0
            )
            temperature = (
                float(row["temperature_k"]) - thermal["cold_wall_temperature_k"]
            ) / (thermal["melting_temperature_k"] - thermal["cold_wall_temperature_k"])
            errors.append(abs(temperature - expected))
            solid.append(1 - float(row["liquid_fraction"]))
        measured_front = math.fsum(solid) * spacing / nx
        observations.append(
            {
                "step": step,
                "temperature_maximum_span_error": max(errors),
                "front_domain_normalized_error": abs(measured_front - front) / height,
                "analytic_front_m": front,
                "original_phase_equivalent_front_m": measured_front,
            }
        )
    return {
        "observations": observations,
        "tolerance": 0.02,
        "passed": all(
            r["temperature_maximum_span_error"] <= 0.02
            and r["front_domain_normalized_error"] <= 0.02
            for r in observations
        ),
        "scope": "one-phase uniform full-water Stefan temperature and solid-phase equivalent front; equal-property fixed-volume synthetic conduction only",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("runtime", "source-reference", "uniform-reference", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    module, path = verifier()
    bridge_path = path.with_name("retained_cooling_bridge.py")
    loader = importlib.util.spec_from_file_location(
        "cooling_package_contract", bridge_path
    )
    # The bridge imports its independent verifier by its packaged sibling name.
    import sys

    sys.path.insert(0, str(path.parent))
    bridge = importlib.util.module_from_spec(loader)
    loader.loader.exec_module(bridge)
    runtime = args.runtime.resolve(strict=True)
    if runtime.parent != Path("/nix/store") or not runtime.is_file():
        raise ValueError("immutable exact packaged native cooling runtime required")
    native = module.common.strict_json(module.read_regular(runtime, 65536))
    closure = Path(native["retained_cooling_closure"]).resolve(strict=True)
    paths = module.read_regular(closure, 2 * 1024**2).decode().splitlines()
    if (
        native["schema_version"] != 1
        or native["backend"] != "cpu"
        or not paths
        or len(paths) != len(set(paths))
        or any(
            Path(p).parent != Path("/nix/store") or not Path(p).exists() for p in paths
        )
        or str(Path(native["retained_cooling"]).parent.parent) not in paths
    ):
        raise ValueError(
            "distinct immutable operation-only closure and CPU adapter required"
        )
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    (root / ".doty-protect").write_text(
        "Preserve exact packaged cooling originals, independent numerical assessments and refused coarse/mutated cases.\n"
    )
    before = service_resources()
    results, rejections, fields = [], [], {}

    def run(name, envelope, raw, layout=False):
        inputs, work = root / ("input-" + name), root / name
        inputs.mkdir(mode=0o700)
        work.mkdir(mode=0o700)
        (inputs / "request.json").write_text(
            json.dumps(envelope, indent=2, allow_nan=False)
        )
        (inputs / "wetting-original.csv").write_bytes(raw)
        if layout:
            (work / "retained-cooling.log").touch(exist_ok=False)
        command = [
            native["bwrap"],
            "--die-with-parent",
            "--new-session",
            "--unshare-all",
            "--dir",
            "/nix/store",
        ]
        for p in paths:
            command += ["--ro-bind", p, p]
        command += [
            "--ro-bind",
            str(closure),
            "/retained-cooling-runtime-closure.txt",
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
            str(work),
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
            "HARBOR_CAD_HOST_NETNS",
            os.readlink("/proc/self/ns/net"),
            "--setenv",
            "HARBOR_CAD_RETAINED_COOLING_POLICY",
            bridge.POLICY,
            native["retained_cooling"],
            "reference",
            "/inputs/request.json",
        ]
        (root / (name + "-command.json")).write_text(json.dumps(command, indent=2))
        with (root / (name + "-launch.log")).open("xb") as log:
            process = subprocess.run(
                command, stdout=log, stderr=subprocess.STDOUT, timeout=950, check=False
            )
        (root / (name + "-exit.txt")).write_text(str(process.returncode) + "\n")
        return process, inputs, work

    cases = []
    for reference, expected in (
        (
            args.source_reference,
            {
                "uniform",
                "retained-s1-t1",
                "retained-s1-t2",
                "retained-s1-t4",
                "retained-s2-t1",
                "retained-s3-t1",
                "retained-s4-t1",
                "retained-s2-t2",
                "retained-s2-t4",
            },
        ),
        (args.uniform_reference, {"uniform-s2", "uniform-s4"}),
    ):
        reference = reference.resolve(strict=True)
        report = module.common.strict_json(
            module.read_regular(reference / "verification.json", 256 * 1024)
        )
        if {row["case"] for row in report["results"]} != expected or len(
            report["results"]
        ) != len(expected):
            raise ValueError(
                "complete separately conserved original reference series required"
            )
        for row in report["results"]:
            name, spec = row["case"], row["request"]
            raw = module.read_regular(
                reference / ("input-" + name) / "wetting-original.csv", 16 * 1024**2
            )
            if (
                hashlib.sha256(raw).hexdigest() != row["source_sha256"]
                or module.common.strict_json(
                    module.read_regular(
                        reference / ("input-" + name) / "request.json", 65536
                    )
                )
                != spec
            ):
                raise ValueError(
                    "unchanged exact original science/source inputs required"
                )
            cases.append((name, spec, raw))
    for name, spec, raw in cases:
        envelope = {
            "schema_version": 1,
            "native_request": spec,
            "original_sha256": hashlib.sha256(raw).hexdigest(),
            "original_bytes": len(raw),
            "maximum_relative_conservation_error": 1e-10,
        }
        bridge.validate_envelope(envelope, raw)
        process, inputs, work = run(
            name, envelope, raw, layout=name == "retained-s2-t1"
        )
        if process.returncode:
            raise RuntimeError(
                f"packaged native cooling failed; exact originals retained at {work}"
            )
        receipt = module.common.strict_json(
            module.read_regular(work / "retained-cooling-receipt.json", 256 * 1024)
        )
        independent = module.verify(spec, raw, receipt, work)
        if (
            receipt["request_sha256"] != checksum(inputs / "request.json")
            or receipt["sandbox"]["policy"] != bridge.POLICY
            or set(receipt["sandbox"]["checks"])
            != {
                "operation_closure_only",
                "no_gpu_nodes",
                "no_sysfs",
                "no_host_home",
                "no_session_bus",
                "no_worker_socket",
                "network_namespace_isolated",
                "descriptor_readonly",
                "original_source_readonly",
            }
            or not all(receipt["sandbox"]["checks"].values())
        ):
            raise ValueError("exact request and complete sandbox canaries required")
        analytic = (
            uniform_analytic(module, spec, raw, work)
            if name.startswith("uniform")
            else None
        )
        results.append(
            {
                "case": name,
                "request": spec,
                "source_sha256": checksum(inputs / "wetting-original.csv"),
                "receipt_sha256": checksum(work / "retained-cooling-receipt.json"),
                "independent": independent,
                "analytic": analytic,
                "native_driver_sha256": receipt["native_driver_sha256"],
                "sandbox": receipt["sandbox"],
                "exit_code": process.returncode,
            }
        )
        if name.startswith("retained"):
            fields[name] = history(module, spec, work)
        (root / "progress.json").write_text(
            json.dumps(results, indent=2, allow_nan=False)
        )
    name, spec, raw = next(row for row in cases if row[0] == "retained-s2-t1")
    base = {
        "schema_version": 1,
        "native_request": spec,
        "original_sha256": hashlib.sha256(raw).hexdigest(),
        "original_bytes": len(raw),
        "maximum_relative_conservation_error": 1e-10,
    }
    for name, envelope in (
        ("rejected-checksum", {**base, "original_sha256": "0" * 64}),
        (
            "rejected-conservation",
            {**base, "maximum_relative_conservation_error": 0.02},
        ),
        ("rejected-execute", {**base, "execute": True}),
        ("rejected-initial", copy.deepcopy(base)),
    ):
        if name == "rejected-initial":
            envelope["native_request"]["thermal"]["initial_temperature_k"] += 1
        process, _, work = run(name, envelope, raw)
        if (
            not process.returncode
            or (work / "process.log").exists()
            or list(work.glob("*.csv"))
        ):
            raise ValueError(
                "invalid native science/source must refuse before dispatch"
            )
        rejections.append(
            {
                "case": name,
                "before_native_launch": True,
                "exit_code": process.returncode,
            }
        )
    span = (
        spec["thermal"]["melting_temperature_k"]
        - spec["thermal"]["cold_wall_temperature_k"]
    )
    spatial = refinements(
        fields, ["retained-s2-t1", "retained-s3-t1", "retained-s4-t1"], span
    )
    temporal = refinements(
        fields, ["retained-s2-t1", "retained-s2-t2", "retained-s2-t4"], span
    )
    coarse = refinements(
        fields, ["retained-s1-t1", "retained-s2-t1", "retained-s4-t1"], span
    )
    if (
        not spatial["passed"]
        or not temporal["passed"]
        or coarse["passed"]
        or any(
            not row["analytic"]["passed"]
            for row in results
            if row["case"] in ("uniform-s2", "uniform-s4")
        )
        or next(row for row in results if row["case"] == "uniform")["analytic"][
            "passed"
        ]
    ):
        raise ValueError(
            "independent fine analytic/history gates and preserved unresolved coarse refusals required"
        )
    report = {
        "schema_version": 1,
        "runtime": str(runtime),
        "runtime_sha256": checksum(runtime),
        "adapter_source_sha256": checksum(bridge_path),
        "verifier_sha256": checksum(path),
        "results": results,
        "rejections": rejections,
        "spatial": spatial,
        "temporal": temporal,
        "unqualified_coarse_spatial": coarse,
        "service_resources_before": before,
        "service_resources_after": service_resources(),
        "package_qualification": "passed_scoped_cpu_native_cooling",
        "worker_qualification": "unqualified",
        "physical_validation": "unqualified",
    }
    (root / "verification.json").write_text(
        json.dumps(report, indent=2, allow_nan=False)
    )
    print(
        json.dumps(
            {
                "cases": len(results),
                "rejections": len(rejections),
                "spatial": spatial["passed"],
                "temporal": temporal["passed"],
                "report_sha256": checksum(root / "verification.json"),
            }
        )
    )


if __name__ == "__main__":
    main()
