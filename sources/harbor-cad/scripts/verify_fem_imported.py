"""Native imported-box static thermal/expansion references; CAD correspondence stays bound."""

import argparse
import hashlib
import importlib.util
import json
import shutil
import subprocess
from pathlib import Path

from verify_cad_mesh import verify_box_mesh
from verify_native_cpu import verify_manifest
from verify_openlb_hip import service_resources


def checksum(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for option in ("runtime", "cad-reference", "output"):
        parser.add_argument(f"--{option}", type=Path, required=True)
    args = parser.parse_args()
    runtime = args.runtime.resolve(strict=True)
    if not runtime.is_relative_to("/nix/store") or not runtime.is_file():
        raise ValueError("exact immutable imported CPU FEM runtime required")
    native = json.loads(runtime.read_text())
    if native["schema_version"] != 1 or native["backend"] != "cpu":
        raise ValueError("explicit native CPU imported FEM stack required")
    reference_path = args.cad_reference / "verification.json"
    qualified = json.loads(reference_path.read_text())
    if len(qualified["results"]) != 6 or len(qualified["rejections"]) != 7:
        raise ValueError(
            "complete origin/translated native imported CAD mesh gate required"
        )
    module_path = Path(__file__).resolve().parents[1] / "adapters/fem_reference.py"
    source = importlib.util.spec_from_file_location("fem_reference", module_path)
    fem = importlib.util.module_from_spec(source)
    source.loader.exec_module(fem)
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    before = service_resources()
    results = []
    for fixture in ("origin", "translated"):
        bundle = args.cad_reference / f"bundle-{fixture}"
        verify_manifest(bundle)
        source_brep = bundle / "solid.brep"
        for resolution in (2, 4, 8):
            geometry = next(
                v["request"]
                for v in qualified["results"]
                if v["fixture"] == fixture and v["resolution"] == resolution
            )
            if geometry["brep_sha256"] != checksum(source_brep):
                raise ValueError("qualified original imported BREP bytes changed")
            for mode in ("thermal_boundary", "free_expansion"):
                ref = {
                    "schema_version": 1,
                    "synthetic": True,
                    "backend": "cpu",
                    "mode": mode,
                    "size_m": [0.02, 0.01, 0.01],
                    "resolution": resolution,
                    "geometry_tolerance_m": geometry["geometry_tolerance_m"],
                    "temperatures_k": [293.15, 303.15],
                    "numerical_tolerance": 1e-6,
                }
                ref.update(
                    {"conductivity_w_m_k": 20.0}
                    if mode == "thermal_boundary"
                    else {
                        "young_modulus_pa": 200e9,
                        "poisson_ratio": 0.3,
                        "expansion_per_k": 12e-6,
                    }
                )
                spec = {
                    "schema_version": 1,
                    "geometry": geometry,
                    "reference": ref,
                    "material_provenance": "explicit synthetic constant-property steel-like reference; no calibration",
                    "boundary_provenance": "prescribed analytical static temperature wall/free expansion, no contact",
                }
                label = f"{fixture}-{mode}-n{resolution}"
                inputs, output = root / f"inputs-{label}", root / label
                inputs.mkdir(mode=0o700)
                output.mkdir(mode=0o700)
                shutil.copyfile(source_brep, inputs / "solid.brep")
                request = inputs / "request.json"
                request.write_text(json.dumps(spec, allow_nan=False, indent=2))
                argv = [
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
                    native["fem_imported"],
                    "reference",
                    "/inputs/request.json",
                ]
                process = subprocess.run(
                    argv, capture_output=True, timeout=150, check=False
                )
                (output / "process.log").write_bytes(process.stdout + process.stderr)
                if process.returncode:
                    raise RuntimeError(f"imported native FEM failed; inspect {output}")
                receipt = json.loads((output / "fem-imported-receipt.json").read_text())
                mesh = json.loads((output / "mesh.json").read_text())
                mesh_checks = verify_box_mesh(geometry, mesh)
                data = (output / "reference.dat").read_bytes()
                nodes = {int(k): v for k, v in mesh["nodes"].items()}
                cells = {int(k): v for k, v in mesh["elements"].items()}
                origin = [geometry["bounds_m"][i] for i in (0, 2, 4)]
                checks = fem.verify(
                    ref, nodes, cells, fem.read_dat(data.decode()), origin=origin
                )
                if (
                    checks != receipt["numerical_verification"]
                    or receipt["world_origin_m"] != origin
                    or receipt["source_transform"] != geometry["source_transform"]
                    or receipt["world_bounds_m"] != geometry["bounds_m"]
                ):
                    raise ValueError(
                        "independent imported-field reference, world origin or original placement changed"
                    )
                if (
                    receipt["request_sha256"] != checksum(request)
                    or receipt["brep_sha256"] != geometry["brep_sha256"]
                    or receipt["mesh_sha256"] != checksum(output / "mesh.json")
                    or receipt["native_field_sha256"]
                    != hashlib.sha256(data).hexdigest()
                ):
                    raise ValueError(
                        "native imported CAD/reference/mesh/field bytes changed"
                    )
                fields = json.loads((output / "imported-fields.json").read_text())
                if (
                    fields["coordinate_unit"] != "m"
                    or fields["world_origin_m"] != origin
                    or not fields["static"]
                    or any(
                        record["physical_time_s"] is not None
                        for records in fields["fields"].values()
                        for record in records
                    )
                ):
                    raise ValueError(
                        "static imported fields invented a physical time or changed coordinate units"
                    )
                if (
                    receipt["software_fallback"]
                    or not receipt["executed"]
                    or receipt["factorization"] != "SPOOLES"
                    or receipt["calculix_version"] != "2.23"
                    or receipt["gmsh_version"] != "4.15.2"
                    or receipt["physical_validation"] != "unqualified"
                ):
                    raise ValueError(
                        "exact native imported reference execution scope changed"
                    )
                results.append(
                    {
                        "fixture": fixture,
                        "resolution": resolution,
                        "mode": mode,
                        "request": spec,
                        "argv": argv,
                        "receipt": receipt,
                        "independent_mesh_checks": mesh_checks,
                        "independent_field_checks": checks,
                    }
                )
    rejections = []
    base = results[0]["request"]
    for name, changed in [
        ("material-provenance", {**base, "material_provenance": ""}),
        ("boundary-provenance", {**base, "boundary_provenance": ""}),
        ("backend", {**base, "reference": {**base["reference"], "backend": "hip"}}),
        (
            "weak-gate",
            {**base, "reference": {**base["reference"], "numerical_tolerance": 1e-4}},
        ),
        (
            "mesh-refinement",
            {**base, "reference": {**base["reference"], "resolution": 8}},
        ),
        (
            "world-dimensions",
            {**base, "reference": {**base["reference"], "size_m": [0.03, 0.01, 0.01]}},
        ),
        (
            "source-bytes",
            {**base, "geometry": {**base["geometry"], "brep_sha256": "0" * 64}},
        ),
        ("contact-injection", {**base, "contact": True}),
    ]:
        inputs, output = root / f"inputs-reject-{name}", root / f"reject-{name}"
        inputs.mkdir(mode=0o700)
        output.mkdir(mode=0o700)
        shutil.copyfile(
            args.cad_reference / "bundle-origin/solid.brep", inputs / "solid.brep"
        )
        (inputs / "request.json").write_text(
            json.dumps(changed, allow_nan=False, indent=2)
        )
        original = results[0]["argv"]
        argv = [
            str(inputs)
            if value == str(root / "inputs-origin-thermal_boundary-n2")
            else str(output)
            if value == str(root / "origin-thermal_boundary-n2")
            else value
            for value in original
        ]
        process = subprocess.run(argv, capture_output=True, timeout=30, check=False)
        (output / "process.log").write_bytes(process.stdout + process.stderr)
        if (
            not process.returncode
            or (output / "mesh.json").exists()
            or (output / "fem-imported-receipt.json").exists()
        ):
            raise ValueError(f"unsupported imported FEM input ran or published: {name}")
        rejections.append(
            {
                "case": name,
                "exit_code": process.returncode,
                "argv": argv,
                "log_sha256": checksum(output / "process.log"),
            }
        )
    report = {
        "schema_version": 1,
        "runtime": str(runtime),
        "runtime_sha256": checksum(runtime),
        "cad_reference": str(reference_path),
        "cad_reference_sha256": checksum(reference_path),
        "results": results,
        "rejections": rejections,
        "service_resources_before": before,
        "service_resources_after": service_resources(),
        "physical_validation": "unqualified",
        "scope": "controlled origin/translated imported BREP static conduction/free expansion; no imported durable FEM job, contact or transient qualification",
    }
    (root / "verification.json").write_text(
        json.dumps(report, allow_nan=False, indent=2)
    )
    print(json.dumps(report, allow_nan=False, indent=2))


if __name__ == "__main__":
    main()
