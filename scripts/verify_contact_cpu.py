"""Exact packaged native planar contact/preload/thermal-opening CPU gate."""

import argparse
import hashlib
import importlib.util
import json
import os
import subprocess
from pathlib import Path

from verify_openlb_hip import service_resources


def checksum(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def request(resolution):
    return {
        "schema_version": 1,
        "synthetic": True,
        "backend": "cpu",
        "formulation": "planar_linear_penalty_contact",
        "size_m": [1e-3] * 3,
        "resolution": resolution,
        "geometry_tolerance_m": 1e-8,
        "initial_gap_m": 0.0,
        "preload_compression_m": 0.5e-6,
        "final_compression_m": 1e-6,
        "young_modulus_pa": [1e8, 1e8],
        "expansion_per_k": [1e-5, 1e-5],
        "reference_temperature_k": 293.15,
        "final_temperatures_k": [293.15, 293.15],
        "contact_stiffness_pa_m": 1e12,
        "numerical_tolerance": 0.002,
        "material_provenance": "controlled synthetic zero-Poisson constant elastic reference",
        "contact_provenance": "prescribed linear penalty pressure/overclosure; not measured gasket data",
        "boundary_provenance": "explicit transverse constraints, bottom support and uniform top displacement",
    }


def load(name, path):
    source = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(source)
    source.loader.exec_module(module)
    return module


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    runtime = args.runtime.resolve(strict=True)
    native = json.loads(runtime.read_text())
    closure = Path(native["contact_closure"])
    if native["backend"] != "cpu" or not runtime.is_relative_to("/nix/store"):
        raise ValueError("exact immutable CPU contact runtime required")
    root = args.output
    root.mkdir(mode=0o700, exist_ok=False)
    bridge = load(
        "contact", Path(__file__).resolve().parents[1] / "adapters/contact_reference.py"
    )
    fem = load("fem", Path(__file__).resolve().parents[1] / "adapters/fem_reference.py")
    before = service_resources()

    def run(name, spec):
        inputs, output = root / (name + "-input"), root / name
        inputs.mkdir(mode=0o700)
        output.mkdir(mode=0o700)
        descriptor = inputs / "request.json"
        descriptor.write_text(json.dumps(spec, indent=2))
        argv = [
            native["bwrap"],
            "--die-with-parent",
            "--new-session",
            "--unshare-all",
            "--dir",
            "/nix",
            "--dir",
            "/nix/store",
        ]
        for mount in closure.read_text().splitlines():
            argv += ["--ro-bind", mount, mount]
        argv += [
            "--ro-bind",
            str(closure),
            "/contact-runtime-closure.txt",
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
            "HARBOR_CAD_CONTACT_POLICY",
            bridge.POLICY,
            "--setenv",
            "HARBOR_CAD_HOST_NETNS",
            os.readlink("/proc/self/ns/net"),
            native["contact"],
            "reference",
            "/inputs/request.json",
        ]
        process = subprocess.run(argv, capture_output=True, check=False, timeout=210)
        log = root / ("launch-" + name + ".log")
        log.write_bytes(process.stdout + process.stderr)
        return process, output, descriptor, argv, log

    results = []
    for case in ("closed", "cooled", "opened", "initial_gap", "dissimilar"):
        for n in (2, 4, 8):
            spec = request(n)
            if case == "cooled":
                spec["final_temperatures_k"] = [273.15, 273.15]
            if case == "opened":
                spec["final_temperatures_k"] = [233.15, 233.15]
            if case == "initial_gap":
                spec["initial_gap_m"] = 0.25e-6
            if case == "dissimilar":
                spec["young_modulus_pa"] = [1e8, 2e8]
                spec["expansion_per_k"] = [1e-5, 2e-5]
                spec["final_temperatures_k"] = [283.15, 283.15]
            bridge.validate(spec)
            process, output, descriptor, argv, log = run(f"{case}-n{n}", spec)
            if process.returncode:
                raise RuntimeError(
                    f"native contact gate failed; original {output} and {log} retained"
                )
            receipt = json.loads((output / "contact-receipt.json").read_text())
            mesh = json.loads((output / "mesh.json").read_text())
            fields = fem.read_dat(
                (output / "reference.dat").read_text(), reaction_forces=True
            )
            nodes, cells = (
                {int(k): v for k, v in mesh[name].items()}
                for name in ("nodes", "elements")
            )
            checks = bridge.verify(
                spec, nodes, cells, mesh["boundary_node_sets"], fields
            )
            assert checks == receipt["numerical_verification"]
            assert receipt["request"] == spec and receipt["request_sha256"] == checksum(
                descriptor
            )
            assert receipt["sandbox"]["policy"] == bridge.POLICY and all(
                receipt["sandbox"]["checks"].values()
            )
            assert (
                receipt["physical_validation"] == "unqualified"
                and mesh["initial_gap_m"] == spec["initial_gap_m"]
            )
            assert all(
                checksum(output / name) == value
                for name, value in receipt["outputs"].items()
            )
            assert len(nodes) == 2 * (n + 1) ** 3 and len(cells) == 2 * n**3
            force = fields["reaction_force"][-1]["values"]
            observed = (
                sum(force[tag,][2] for tag in mesh["boundary_node_sets"]["bottom"])
                / 1e-6
            )
            results.append(
                {
                    "case": case,
                    "resolution": n,
                    "request": spec,
                    "receipt": receipt,
                    "observed_pressure_pa": observed,
                    "receipt_sha256": checksum(output / "contact-receipt.json"),
                    "argv": argv,
                }
            )
    refinements = []
    for case in sorted({v["case"] for v in results}):
        selected = [v for v in results if v["case"] == case]
        pressures = [v["observed_pressure_pa"] for v in selected]
        scale = max(
            1.0, abs(bridge.reference(selected[0]["request"], 2)["pressure_pa"])
        )
        spread = (max(pressures) - min(pressures)) / scale
        assert spread <= 0.002
        refinements.append(
            {
                "case": case,
                "resolutions": [2, 4, 8],
                "observed_pressure_pa": pressures,
                "relative_pressure_spread": spread,
                "tolerance": 0.002,
                "passed": True,
                "scope": "spatial mesh-independence at fixed planar analytic law; no continuum-rate inference",
            }
        )
    rejections = []
    for key, value in (
        ("backend", "hip"),
        ("synthetic", False),
        ("poisson_ratio", 0.3),
        ("initial_gap_m", -1e-6),
        ("resolution", 32),
        ("numerical_tolerance", 0.01),
        ("contact_stiffness_pa_m", 0),
        ("final_temperatures_k", [0, 293]),
    ):
        spec = {**request(2), key: value}
        process, output, _, _, log = run("reject-" + key, spec)
        assert process.returncode and not list(output.iterdir())
        rejections.append(
            {"input": key, "log_sha256": checksum(log), "scientific_output_empty": True}
        )
    report = {
        "schema_version": 1,
        "runtime": str(runtime),
        "runtime_sha256": checksum(runtime),
        "results": results,
        "refinements": refinements,
        "rejections": rejections,
        "service_resources_before": before,
        "service_resources_after": service_resources(),
        "physical_validation": "unqualified",
        "scope": "synthetic CPU two-block zero-Poisson planar linear penalty contact/preload and uniform prescribed thermal expansion; no gasket seal, measured cold properties or hybrid FEM",
    }
    path = root / "verification.json"
    path.write_text(json.dumps(report, indent=2, allow_nan=False))
    print(
        json.dumps(
            {
                "report_sha256": checksum(path),
                "solves": len(results),
                "rejections": len(rejections),
                "refinements": refinements,
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
