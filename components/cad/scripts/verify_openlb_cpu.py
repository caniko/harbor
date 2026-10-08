"""Run the real CPU driver on a labelled procedural STL and verify retained VTK.

This fixture is independent of FreeCAD. Passing it establishes a bounded numerical
reference only, never CAD, GPU, sandbox, or physical qualification.
"""

import argparse
import array
import base64
import json
import math
import struct
import subprocess
import sys
import xml.etree.ElementTree as ET
import zlib
from pathlib import Path


def box_stl(path):
    # Millimetres, matching the CAD export contract, with outward-facing triangles.
    vertices = [(x, y, z) for z in (0, 10) for y in (0, 10) for x in (0, 20)]
    faces = [
        (0, 2, 3, 1),
        (4, 5, 7, 6),
        (0, 1, 5, 4),
        (2, 6, 7, 3),
        (0, 4, 6, 2),
        (1, 3, 7, 5),
    ]
    lines = ["solid synthetic_procedural_channel"]
    for a, b, c, d in faces:
        for triangle in ((a, b, c), (a, c, d)):
            lines.extend(["facet normal 0 0 0", "outer loop"])
            lines.extend("vertex " + " ".join(map(str, vertices[i])) for i in triangle)
            lines.extend(["endloop", "endfacet"])
    path.write_text("\n".join(lines + ["endsolid synthetic_procedural_channel"]))


def read_vti(path):
    root = ET.parse(path).getroot()
    if root.attrib.get("compressor") != "vtkZLibDataCompressor":
        raise ValueError("expected lossless zlib VTK encoding")
    image = root.find("ImageData")
    piece = image.find("Piece")
    extent = list(map(int, piece.attrib["Extent"].split()))
    shape = [extent[i + 1] - extent[i] + 1 for i in (0, 2, 4)]
    points = math.prod(shape)
    fields = {}
    for field in piece.findall("PointData/DataArray"):
        if field.attrib["type"] != "Float64":
            raise ValueError("scientific Float64 arrays must not be downcast")
        encoded = "".join(field.text.split())
        blocks, size, last, compressed = struct.unpack(
            "<4I", base64.b64decode(encoded[:24])
        )
        if blocks != 1 or size != last:
            raise ValueError("unexpected VTK compression header")
        payload = base64.b64decode(encoded[24:])
        if len(payload) != compressed:
            raise ValueError("VTK compressed length mismatch")
        raw = zlib.decompress(payload)
        components = int(field.attrib["NumberOfComponents"])
        if len(raw) != size or len(raw) != points * components * 8:
            raise ValueError(f"Float64 payload length mismatch: {field.attrib['Name']}")
        values = array.array("d")
        values.frombytes(raw)
        if sys.byteorder != "little":
            values.byteswap()
        if any(not math.isfinite(value) for value in values):
            raise ValueError("nonfinite retained scientific field")
        fields[field.attrib["Name"]] = (components, values)
    return image, extent, shape, fields


def prepare_fixture(root, resolution, selection=None):
    directory = root / f"resolution-{resolution}"
    directory.mkdir(mode=0o700)
    box_stl(directory / "fluid.stl")
    plan = {
        "case": {
            "geometry": {"synthetic": True},
            "applicability": {
                "formulation": "periodic_forced_channel",
                "numerical_tolerance": 0.05,
            },
            "length": {"value": 0.02, "unit": "m"},
            "channel_height": {"value": 0.01, "unit": "m"},
            "kinematic_viscosity": {"value": 1e-5, "unit": "m2/s"},
            "acceleration": {"value": 0.001, "unit": "m/s2"},
            "material": {"density": {"value": 1.0, "unit": "kg/m3"}},
            "resolution": resolution,
            "max_time_s": 20.0,
        },
        "observation": {"retained_times_s": [0.0, 10.0, 20.0]},
    }
    if selection is not None:
        plan["stages"] = [
            {"operation": "openlb", "gpu": "required", "selection": selection}
        ]
    (directory / "plan.json").write_text(json.dumps(plan))
    return directory


def run(executable, root, resolution, selection=None, env=None):
    directory = prepare_fixture(root, resolution, selection)
    with (directory / "process.log").open("w") as log:
        subprocess.run(
            [executable, "openlb", "plan.json"],
            cwd=directory,
            stdout=log,
            stderr=subprocess.STDOUT,
            check=True,
            timeout=180,
            env=env,
        )
    return verify_fields(directory, resolution, selection)


def verify_fields(directory, resolution, selection=None):
    receipt = json.loads((directory / "openlb-receipt.json").read_text())
    backend = "cpu" if selection is None else selection["backend"]
    if receipt["backend"] != backend or not receipt["executed"]:
        raise ValueError(
            "reference must exercise the explicitly selected OpenLB backend"
        )
    if selection is not None and (
        receipt["pci"] != selection["pci"]
        or receipt["backend_uuid"] != selection["backend_uuid"]
        or receipt.get("software_fallback") is not False
        or receipt.get("gpu_kernel_completion_verified") is not True
        or receipt.get("gpu_blocks", 0) <= 0
    ):
        raise ValueError(
            "GPU reference lacks exact device/block/kernel execution evidence"
        )
    collection_path = directory / "tmp/vtkData/channel.pvd"
    collection = ET.parse(collection_path).findall("Collection/DataSet")
    mappings = receipt["retained_times"]
    if [int(item.attrib["timestep"]) for item in collection] != [
        m["step"] for m in mappings
    ]:
        raise ValueError("retained physical time / lattice-step collection mismatch")
    last_fields = None
    for item in collection:
        multiblock_path = collection_path.parent / item.attrib["file"]
        for dataset in ET.parse(multiblock_path).findall(".//DataSet"):
            last_fields = read_vti(multiblock_path.parent / dataset.attrib["file"])
    image, extent, shape, fields = last_fields
    nx, ny, nz = shape
    origin = list(map(float, image.attrib["Origin"].split()))
    spacing = list(map(float, image.attrib["Spacing"].split()))
    components, velocity = fields["physVelocity"]
    if components != 3:
        raise ValueError("expected point-associated 3D velocity")
    samples = []
    for j in range(ny):
        y = origin[1] + (extent[2] + j) * spacing[1]
        if 0 < y < 0.01:
            point = (nz // 2 * ny + j) * nx + nx // 2
            observed = velocity[point * 3]
            expected = 0.001 * y * (0.01 - y) / (2 * 1e-5)
            samples.append((observed, expected))
    error = math.sqrt(
        sum((a - b) ** 2 for a, b in samples) / sum(b**2 for _, b in samples)
    )
    if error > 0.05:
        raise ValueError(f"analytical channel relative L2 error {error} exceeds 0.05")
    return {
        "resolution": resolution,
        "receipt": receipt,
        "relative_l2_error": error,
        "vtk_float64_payloads": "verified",
        "physical_validation": "unqualified",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    results = [
        run(str(Path(args.executable).resolve()), args.output, n) for n in (8, 16)
    ]
    if results[1]["relative_l2_error"] >= results[0]["relative_l2_error"]:
        raise ValueError("refinement must reduce analytical error")
    rejected = args.output / "rejected-gpu-plan"
    rejected.mkdir()
    plan = json.loads((args.output / "resolution-8/plan.json").read_text())
    plan["stages"] = [
        {
            "operation": "openlb",
            "gpu": "required",
            "selection": {
                "role": "compute",
                "backend": "hip",
                "pci": "0000:03:00.0",
                "backend_uuid": "GPU-fixture",
            },
        }
    ]
    (rejected / "plan.json").write_text(json.dumps(plan))
    output = subprocess.run(
        [str(Path(args.executable).resolve()), "openlb", "plan.json"],
        cwd=rejected,
        capture_output=True,
        text=True,
        timeout=30,
        check=False,
    )
    (rejected / "process.log").write_text(output.stdout + output.stderr)
    if (
        output.returncode == 0
        or "CPU driver cannot execute" not in output.stderr
        or (rejected / "tmp").exists()
    ):
        raise ValueError(
            "CPU driver did not reject the required GPU plan before solver output"
        )
    report = {
        "fixture": "synthetic procedural STL; not FreeCAD evidence",
        "results": results,
        "required_gpu_plan_rejected": True,
    }
    (args.output / "verification.json").write_text(
        json.dumps(report, indent=2, allow_nan=False)
    )
    print(json.dumps(report, indent=2, allow_nan=False))


if __name__ == "__main__":
    main()
