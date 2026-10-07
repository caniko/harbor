"""Native UV planar/angular irradiance and exact optical-dose qualification."""

import argparse
import copy
import csv
import hashlib
import importlib.util
import json
import math
import os
import subprocess
import sys
from itertools import pairwise
from pathlib import Path

from verify_openlb_hip import service_resources


def checksum(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("executable", "output"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    native = parser.add_mutually_exclusive_group(required=True)
    native.add_argument("--runtime", type=Path)
    native.add_argument(
        "--development-site",
        type=Path,
        help="Explicitly unqualified mutable exact-wheel diagnostic",
    )
    parser.add_argument("--development-planner", action="store_true")
    args = parser.parse_args()
    binary = args.executable.resolve(strict=True)
    if not binary.is_file() or (
        not args.development_planner and not binary.is_relative_to("/nix/store")
    ):
        raise ValueError(
            "exact immutable planner or explicit source-built diagnostic required"
        )
    repo = Path(__file__).resolve().parents[1]
    module = importlib.util.spec_from_file_location(
        "independent_spectral_verifier", repo / "adapters/spectral_reference.py"
    )
    verifier = importlib.util.module_from_spec(module)
    module.loader.exec_module(verifier)
    before = service_resources()
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    (root / ".doty-protect").write_text(
        "Active native UV original observations and failed attempts; preserve.\n"
    )
    runtime = None
    paths = []
    if args.runtime:
        descriptor = args.runtime.resolve(strict=True)
        if not descriptor.is_relative_to("/nix/store"):
            raise ValueError(
                "immutable native spectral CPU runtime descriptor required"
            )
        runtime = json.loads(descriptor.read_text())
        if (
            runtime["schema_version"] != 1
            or runtime["backend"] != "cpu"
            or runtime["precision"] != "Float32"
        ):
            raise ValueError("exact scalar spectral CPU/precision route required")
        closure = Path(runtime["spectral_closure"]).resolve(strict=True)
        paths = closure.read_text().splitlines()
        if (
            not paths
            or len(paths) != len(set(paths))
            or any(
                Path(p).parent != Path("/nix/store") or not Path(p).exists()
                for p in paths
            )
            or Path(runtime["spectral"]).parent.parent.as_posix() not in paths
        ):
            raise ValueError("complete bounded immutable operation closure required")
    else:
        site = args.development_site.resolve(strict=True)
        identity = json.loads((site.parent / "identity.json").read_text())
        pins = {record["name"]: record["sha256"] for record in identity["pins"]}
        if pins != {
            "mitsuba": "8959e8de33427cf4d9b515a52d741dca7624e7c17094c5849f123a96504ca123",
            "drjit": "33a4b146cc56a02ea0dd9c43034277c59ae3c5dd3490678c2cbc8ef69a1e8c93",
        }:
            raise ValueError("exact development native ABI identities required")
        if any(
            checksum(site / path) != digest
            for path, digest in identity["files"].items()
        ):
            raise ValueError("unchanged development wheel/ABI extraction required")
    base = json.loads((repo / "examples/spectral-reference.json").read_text())
    cases = []
    for name, normal, occlusion in (
        ("normal", [0, 0, 1], "none"),
        ("inclined", [0.6, 0, 0.8], "none"),
        ("back", [0, 0, -1], "none"),
        ("blocked", [0, 0, 1], "full_directional_occluder"),
    ):
        spec = copy.deepcopy(base)
        spec.update(sensor_normal=normal, occlusion=occlusion)
        cases.append((name, spec))
    diffuse = copy.deepcopy(base)
    diffuse["source"] = {
        "kind": "isotropic",
        "radiance": [
            {"value": 1.0, "unit": "W/(m2*sr*nm)"},
            {"value": 3.0, "unit": "W/(m2*sr*nm)"},
        ],
    }
    for samples in (4096, 16384, 65536):
        spec = copy.deepcopy(diffuse)
        spec["samples"] = samples
        cases.append((f"isotropic-s{samples}", spec))
    area = copy.deepcopy(base)
    area["sensor_width"]["value"] *= 2
    cases.append(("double-area", area))
    reflection = json.loads(
        (repo / "examples/spectral-reflection-reference.json").read_text()
    )
    cases.append(("reflection-rho04", reflection))
    high = copy.deepcopy(reflection)
    high["reflectance"] = 0.8
    cases.append(("reflection-rho08", high))
    black = copy.deepcopy(reflection)
    black["reflectance"] = 0.0
    black["disk_radius"]["value"] = 1.0
    black["incident"]["sensor_width"] = {"value": 1e-5, "unit": "m"}
    black["incident"]["sensor_height"] = {"value": 1e-5, "unit": "m"}
    cases.append(("reflection-black", black))
    records = []
    for name, spec in cases:
        incident = spec.get("incident", spec)
        inputs = root / f"input-{name}"
        inputs.mkdir(mode=0o700)
        request = inputs / "request.json"
        request.write_text(json.dumps(spec, allow_nan=False, separators=(",", ":")))
        prepared = json.loads(
            subprocess.check_output(
                [
                    str(binary),
                    "case",
                    "validate-spectral-reflection-reference"
                    if "incident" in spec
                    else "validate-spectral-reference",
                    str(request),
                ],
                timeout=30,
            )
        )
        normalized = verifier.normalize(spec)
        for channel, key in (
            ("incident", "incident_irradiance_w_m2"),
            ("absorbed", "absorbed_irradiance_w_m2"),
            ("ageing", "ageing_weighted_irradiance_w_m2"),
        ):
            if not math.isclose(
                prepared[key],
                normalized["reference"][channel],
                rel_tol=1e-14,
                abs_tol=1e-12,
            ):
                raise ValueError(
                    "independent Rust/Python spectral-product quadrature must agree"
                )
        work = root / name
        work.mkdir(mode=0o700)
        environment = {**os.environ, "PYTHONDONTWRITEBYTECODE": "1"}
        if runtime:
            command = [
                runtime["bwrap"],
                "--die-with-parent",
                "--new-session",
                "--unshare-all",
                "--dir",
                "/nix/store",
            ]
            for path in paths:
                command.extend(["--ro-bind", path, path])
            command.extend(
                [
                    "--ro-bind",
                    str(closure),
                    "/spectral-runtime-closure.txt",
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
                    "HARBOR_CAD_SPECTRAL_POLICY",
                    "harbor-cad-spectral-cpu-v1",
                    "--setenv",
                    "HARBOR_CAD_HOST_NETNS",
                    os.readlink("/proc/self/ns/net"),
                    runtime["spectral"],
                    "reference",
                    "/inputs/request.json",
                ]
            )
        else:
            environment["PYTHONPATH"] = str(site)
            command = [
                sys.executable,
                "-B",
                str(repo / "adapters/spectral_reference.py"),
                "reference",
                str(request),
            ]
        with (root / f"launch-{name}.log").open("xb") as log:
            process = subprocess.run(
                command,
                cwd=work,
                env=environment,
                stdout=log,
                stderr=subprocess.STDOUT,
                timeout=300,
                check=False,
            )
        (root / f"exit-{name}.txt").write_text(str(process.returncode))
        if process.returncode:
            raise RuntimeError(
                f"native spectral gate failed; original fields/logs retained at {work}"
            )
        receipt = json.loads((work / "spectral-receipt.json").read_text())
        if (
            receipt["input"] != spec
            or receipt["request_sha256"] != checksum(request)
            or receipt["variant"] != "scalar_spectral"
            or receipt["precision"] != "Float32"
            or receipt["backend"] != "cpu"
            or receipt["executed"] is not True
            or receipt["software_fallback"] is not False
            or receipt["normalized"] != normalized
            or [obs["seed"] for obs in receipt["observations"]] != incident["seeds"]
        ):
            raise ValueError(
                "exact native source, variant, scientific identity and complete retained seed coverage required"
            )
        if runtime and (
            receipt["sandbox"]["policy"] != "harbor-cad-spectral-cpu-v1"
            or len(receipt["sandbox"]["checks"]) != 8
            or not all(receipt["sandbox"]["checks"].values())
        ):
            raise ValueError(
                "measured complete closure-only CPU sandbox canaries required"
            )
        for observation in receipt["observations"]:
            field = work / observation["path"]
            if (
                checksum(field) != observation["sha256"]
                or field.stat().st_size != observation["bytes"]
                or observation["samples"] != incident["samples"]
            ):
                raise ValueError(
                    "closed unchanged native spectral observations required"
                )
            actual = observation["native_channels_w_m2"]
            if incident["source"]["kind"] == "directional":
                with field.open() as handle:
                    rows = list(csv.DictReader(handle))
                if len(rows) != incident["samples"] or [
                    int(row["sample"]) for row in rows
                ] != list(range(incident["samples"])):
                    raise ValueError(
                        "complete native directional surface observations required"
                    )
                means = [
                    math.fsum(
                        float(row[f"emitter_weight_w_m2_nm_{i}"])
                        * float(row["native_cosine"])
                        for row in rows
                    )
                    / len(rows)
                    for i in range(len(normalized["wavelengths_nm"]))
                ]
                reconstructed = {
                    channel: verifier.product_integral(
                        normalized["wavelengths_nm"], means, weights
                    )
                    for channel, weights in normalized["weights"].items()
                }
                if any(
                    not math.isclose(value, actual[key], rel_tol=1e-12, abs_tol=1e-12)
                    for key, value in reconstructed.items()
                ):
                    raise ValueError(
                        "independent complete-original native directional weight integration required"
                    )
            else:
                if runtime:
                    inspection = command[:-2] + ["inspect-exr", f"/work/{field.name}"]
                else:
                    inspection = [
                        sys.executable,
                        "-B",
                        str(repo / "adapters/spectral_reference.py"),
                        "inspect-exr",
                        str(field),
                    ]
                observed = json.loads(
                    subprocess.check_output(
                        inspection, cwd=work, env=environment, timeout=30
                    )
                )
                if observed != actual:
                    raise ValueError(
                        "separate-process original Float32 spectral EXR observations must match exact reported channels"
                    )
            checks = verifier.verify_channels(
                normalized, actual, incident["relative_tolerance"]
            )
            if checks != observation["numerical_verification"] or any(
                not math.isclose(
                    observation["exposure_j_m2"][key],
                    value * normalized["history_integral_s"],
                    rel_tol=1e-14,
                    abs_tol=1e-12,
                )
                for key, value in actual.items()
            ):
                raise ValueError(
                    "unchanged optical/numerical and prescribed temporal exposure gates required"
                )
        records.append(
            {
                "case": name,
                "preparation": prepared,
                "receipt": receipt,
                "original_files_sha256": {
                    p.name: checksum(p) for p in work.iterdir() if p.is_file()
                },
                "argv": command,
                "exit_code": process.returncode,
            }
        )
        if "reflection" in normalized:
            if receipt["reflection_model_assessment"] != normalized["reflection"]:
                raise ValueError(
                    "independent finite-footprint/shadow reflection model assessment required"
                )
            for key in (
                "disk_view_factor",
                "reflection_factor",
                "model_relative_error_bound",
                "black_sensor_shadow_relative_error_bound",
            ):
                if not math.isclose(
                    prepared[key],
                    normalized["reflection"][key],
                    rel_tol=1e-10,
                    abs_tol=1e-15,
                ):
                    raise ValueError(
                        "independent Rust/Python projected-solid-angle and model-error reference agreement required"
                    )
    rms = []
    for samples in (4096, 16384, 65536):
        record = next(
            record for record in records if record["case"] == f"isotropic-s{samples}"
        )
        errors = [
            check["relative_error"]
            for observation in record["receipt"]["observations"]
            for check in observation["numerical_verification"]["channels"].values()
        ]
        rms.append(
            math.sqrt(math.fsum(value * value for value in errors) / len(errors))
        )
    if any(a <= b for a, b in pairwise(rms)):
        raise ValueError(
            f"fixed-spectrum independent-seed numerical sampling error must decrease under equal-reference refinement: {rms}"
        )
    rejection_specs = []
    for field, value in (
        ("precision", "Float64"),
        ("variant", "cuda_ad_spectral"),
        ("samples", 4095),
        ("relative_tolerance", 0.1),
        ("absorptivity", [0.2, 1.1]),
        ("source_provenance", ""),
        ("reflection", None),
    ):
        invalid = copy.deepcopy(base)
        invalid[field] = value
        rejection_specs.append((field, invalid, "validate-spectral-reference"))
    for field, value in (
        ("maximum_model_error", 0.02),
        ("reflectance", 1.1),
        ("reflectance_provenance", ""),
        ("reflection", None),
    ):
        invalid = copy.deepcopy(reflection)
        invalid[field] = value
        rejection_specs.append(
            ("reflection-" + field, invalid, "validate-spectral-reflection-reference")
        )
    rejections = []
    for name, spec, cli_command in rejection_specs:
        invalid_inputs = root / f"reject-input-{name}"
        invalid_inputs.mkdir(mode=0o700)
        invalid_request = invalid_inputs / "request.json"
        invalid_request.write_text(json.dumps(spec, allow_nan=False))
        invalid_work = root / f"reject-{name}"
        invalid_work.mkdir(mode=0o700)
        replacements = {
            str(inputs): str(invalid_inputs),
            str(request): str(invalid_request),
            str(work): str(invalid_work),
        }
        native_command = [replacements.get(argument, argument) for argument in command]
        result = subprocess.run(
            native_command,
            cwd=invalid_work,
            env=environment,
            capture_output=True,
            timeout=30,
            check=False,
        )
        (root / f"reject-{name}.log").write_bytes(result.stdout + result.stderr)
        cli = subprocess.run(
            [str(binary), "case", cli_command, str(invalid_request)],
            capture_output=True,
            timeout=30,
            check=False,
        )
        if (
            result.returncode == 0
            or cli.returncode == 0
            or any(invalid_work.iterdir())
            or b"ValueError" not in result.stderr
            or b"invalid_input" not in cli.stderr
        ):
            raise ValueError(
                "strict native/CLI applicability rejection must precede native output and preserve typed errors"
            )
        rejections.append(
            {
                "case": name,
                "argv": native_command,
                "exit_code": result.returncode,
                "cli_exit_code": cli.returncode,
                "request_sha256": checksum(invalid_request),
                "log_sha256": checksum(root / f"reject-{name}.log"),
                "native_output_files": 0,
            }
        )
    report = {
        "schema_version": 1,
        "planner": str(binary),
        "planner_sha256": checksum(binary),
        "planner_qualification": "unqualified development executable"
        if args.development_planner
        else "exact immutable package",
        "native_qualification": "unqualified mutable wheel diagnostic"
        if not runtime
        else "exact immutable standalone CPU reference; worker unqualified",
        "runtime": str(args.runtime) if runtime else None,
        "runtime_sha256": checksum(args.runtime) if runtime else None,
        "adapter_source_sha256": checksum(repo / "adapters/spectral_reference.py"),
        "results": records,
        "rejections": rejections,
        "isotropic_sampling_error_rms_4096_16384_65536": rms,
        "service_resources_before": before,
        "service_resources_after": service_resources(),
        "scope": "synthetic native UV surface irradiance, angular orientation, opaque occlusion, bounded Lambertian disk reflection, separate optical weights and prescribed exact amplitude dose; no atmosphere, GPU, worker or physical qualification",
        "physical_validation": "unqualified",
    }
    (root / "verification.json").write_text(
        json.dumps(report, allow_nan=False, indent=2)
    )
    print(
        json.dumps(
            {
                "report_sha256": checksum(root / "verification.json"),
                "cases": len(records),
                "observations": sum(
                    len(record["receipt"]["observations"]) for record in records
                ),
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
