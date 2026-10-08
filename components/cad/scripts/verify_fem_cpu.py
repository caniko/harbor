"""Bounded packaged synthetic Gmsh/CalculiX references, separate from worker/CAD qualification."""

import argparse
import hashlib
import json
import subprocess
from pathlib import Path

from verify_openlb_hip import service_resources


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    resources = service_resources()
    runtime = args.runtime.resolve(strict=True)
    if not runtime.is_relative_to("/nix/store") or not runtime.is_file():
        raise ValueError("immutable packaged FEM reference runtime required")
    native = json.loads(runtime.read_text())
    if native["schema_version"] != 1 or native["backend"] != "cpu":
        raise ValueError("explicit CPU reference runtime required")
    binary = Path(native["fem"]).resolve(strict=True)
    if not binary.is_relative_to("/nix/store") or not binary.is_file():
        raise ValueError("immutable packaged FEM adapter required")
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    inputs = root / "inputs"
    inputs.mkdir(mode=0o700)

    def invoke(label, spec):
        path = inputs / f"{label}.json"
        path.write_text(json.dumps(spec, allow_nan=False, indent=2))
        output = root / label
        output.mkdir(mode=0o700)
        command = [
            native["bwrap"],
            "--die-with-parent",
            "--new-session",
            "--unshare-all",
            "--ro-bind",
            "/nix/store",
            "/nix/store",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--tmpfs",
            "/tmp",
            "--tmpfs",
            "/home",
            "--dir",
            "/home/native",
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
            "/home/native",
            "--setenv",
            "LC_ALL",
            "C",
            "--setenv",
            "OMP_NUM_THREADS",
            "1",
            str(binary),
            "reference",
            f"/inputs/{path.name}",
        ]
        process = subprocess.run(
            command, capture_output=True, text=True, timeout=135, check=False
        )
        (output / "process.log").write_text(process.stdout + process.stderr)
        return output, process, command

    base = {
        "schema_version": 1,
        "synthetic": True,
        "backend": "cpu",
        "mode": "thermal_boundary",
        "size_m": [0.02, 0.01, 0.01],
        "resolution": 4,
        "geometry_tolerance_m": 1e-6,
        "temperatures_k": [293.15, 303.15],
        "numerical_tolerance": 1e-6,
        "conductivity_w_m_k": 20.0,
    }
    results = []
    for mode in ("thermal_boundary", "free_expansion"):
        for refinement in (2, 4, 8):
            spec = {**base, "mode": mode, "resolution": refinement}
            if mode == "free_expansion":
                del spec["conductivity_w_m_k"]
                spec.update(
                    young_modulus_pa=200e9, poisson_ratio=0.3, expansion_per_k=12e-6
                )
            output, process, command = invoke(f"{mode}-{refinement}", spec)
            if process.returncode:
                raise RuntimeError(
                    f"native reference failed: {output}\n{process.stderr[-4000:]}"
                )
            receipt = json.loads((output / "fem-reference-receipt.json").read_text())
            if (
                receipt["backend"] != "cpu"
                or receipt["factorization"] != "SPOOLES"
                or not receipt["executed"]
                or receipt["software_fallback"]
                or receipt["calculix_version"] != "2.23"
                or receipt["gmsh_version"] != "4.15.2"
            ):
                raise ValueError("native reference did not execute the exact CPU stack")
            mesh = json.loads((output / "mesh.json").read_text())
            if (
                receipt["elements"] != refinement**3
                or receipt["nodes"] != (refinement + 1) ** 3
                or not mesh["positive_gauss_jacobians"]
            ):
                raise ValueError("reference refinement or positive topology changed")
            expected = (
                {"temperature", "heat_flux"}
                if mode == "thermal_boundary"
                else {"displacement", "stress"}
            )
            checks = receipt["numerical_verification"]
            if set(checks) != expected or any(
                not c["passed"]
                or c["normalized_max_abs_error"] > 1e-6
                or c["tolerance"] != 1e-6
                for c in checks.values()
            ):
                raise ValueError("unchanged independent analytical gates failed")
            files = {}
            for path in sorted(output.iterdir()):
                if path.is_symlink() or not path.is_file():
                    raise ValueError("regular native output tree required")
                if path.stat().st_size > 64 * 1024**2:
                    raise ValueError("native reference artifact budget")
                files[path.name] = {
                    "bytes": path.stat().st_size,
                    "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                }
            results.append(
                {
                    "request": spec,
                    "argv": command,
                    "receipt": receipt,
                    "artifacts": files,
                }
            )
    rejections = []
    for key, value in [
        ("backend", "hip"),
        ("synthetic", False),
        ("resolution", 100000),
        ("temperatures_k", [-1, 303.15]),
        ("conductivity_w_m_k", 0),
        ("numerical_tolerance", 0.1),
        ("mode", "contact"),
    ]:
        output, process, command = invoke(f"reject-{key}", {**base, key: value})
        if (
            not process.returncode
            or (output / "reference.msh").exists()
            or (output / "fem-reference-receipt.json").exists()
        ):
            raise ValueError(
                "unsupported input ran or silently substituted a reference"
            )
        rejections.append(
            {
                "field": key,
                "exit_code": process.returncode,
                "argv": command,
                "reason": process.stderr[-2000:],
            }
        )
    report = {
        "schema_version": 1,
        "scope": "independent packaged synthetic CPU FEM references; no worker/CAD/hybrid-GPU qualification",
        "runtime": str(runtime),
        "runtime_sha256": hashlib.sha256(runtime.read_bytes()).hexdigest(),
        "adapter": str(binary),
        "adapter_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "service_resources_before": resources,
        "service_resources_after": service_resources(),
        "results": results,
        "rejections": rejections,
        "convergence": "analytical consistency at three refinements; no transient convergence claim",
        "physical_validation": "unqualified",
    }
    (root / "verification.json").write_text(
        json.dumps(report, allow_nan=False, indent=2)
    )
    print(json.dumps(report, allow_nan=False, indent=2))


if __name__ == "__main__":
    main()
