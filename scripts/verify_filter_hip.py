"""Bounded native HIP-gradient references, distinct from worker/B2 qualification.

Run in an owned <=2 GiB/no-swap, <=2 CPU/128-task service. This exercises linear,
quadratic and retained OpenLB Float64 fields, explicit CPU/HIP comparisons and
native no-fallback rejection. Worker lifecycle/isolation requires its own gate.
"""

import argparse
import array
import base64
import fcntl
import hashlib
import json
import math
import os
import pwd
import re
import sqlite3
import stat
import struct
import subprocess
import sys
import time
import xml.etree.ElementTree as ET
from pathlib import Path

from verify_openlb_cpu import read_vti
from verify_openlb_hip import service_resources


def digest(data):
    return hashlib.sha256(data).hexdigest()


def synthetic_image(path, quadratic):
    # Signed extents reproduce valid OpenLB halo indices. Anisotropic coordinates
    # and cross-axis terms expose spacing/component/derivative-order mistakes.
    extent = [-1, 14, -2, 13, -3, 12]
    origin, spacing = [0.125, -0.25, 0.375], [0.03125, 0.0625, 0.125]
    velocity, pressure, geometry, expected = [], [], [], []
    for k in range(extent[4], extent[5] + 1):
        for j in range(extent[2], extent[3] + 1):
            for i in range(extent[0], extent[1] + 1):
                xyz = [o + index * h for o, index, h in zip(origin, (i, j, k), spacing)]
                coefficients = (
                    [(1, 2, 3), (4, -5, 6), (-7, 8, 9)]
                    if quadratic
                    else [(2, 3, 5), (-7, 11, 13), (17, 19, -23)]
                )
                gradient = []
                for coefficients_row in coefficients:
                    velocity.append(
                        sum(
                            a * x ** (2 if quadratic else 1)
                            for a, x in zip(coefficients_row, xyz)
                        )
                    )
                    gradient.extend(
                        2 * a * x if quadratic else a
                        for a, x in zip(coefficients_row, xyz)
                    )
                pressure_coefficients = (10, 11, -12) if quadratic else (29, -31, 37)
                pressure.append(
                    sum(
                        a * x ** (2 if quadratic else 1)
                        for a, x in zip(pressure_coefficients, xyz)
                    )
                )
                geometry.append(1 + ((i + j + k) % 3))
                interior = all(
                    extent[n] < index < extent[n + 1]
                    for n, index in zip((0, 2, 4), (i, j, k))
                )
                pressure_gradient = [
                    2 * a * x if quadratic else a
                    for a, x in zip(pressure_coefficients, xyz)
                ]
                expected.append(
                    (
                        interior or not quadratic,
                        {"physVelocity": gradient, "physPressure": pressure_gradient},
                    )
                )
    root = ET.Element(
        "VTKFile",
        type="ImageData",
        version="1.0",
        byte_order="LittleEndian",
        header_type="UInt32",
    )
    attributes = {
        "WholeExtent": " ".join(map(str, extent)),
        "Origin": " ".join(map(str, origin)),
        "Spacing": " ".join(map(str, spacing)),
    }
    image = ET.SubElement(root, "ImageData", attributes)
    piece = ET.SubElement(image, "Piece", Extent=attributes["WholeExtent"])
    point = ET.SubElement(piece, "PointData")
    ET.SubElement(piece, "CellData")
    for name, components, values in [
        ("physVelocity", 3, velocity),
        ("physPressure", 1, pressure),
        ("geometry", 1, geometry),
    ]:
        payload = struct.pack(f"<{len(values)}d", *values)
        field = ET.SubElement(
            point,
            "DataArray",
            type="Float64",
            Name=name,
            NumberOfComponents=str(components),
            format="binary",
        )
        field.text = (
            base64.b64encode(struct.pack("<I", len(payload))).decode()
            + base64.b64encode(payload).decode()
        )
    path.write_bytes(ET.tostring(root, encoding="utf-8", xml_declaration=True))
    return expected


def read_gradient(path):
    data = path.read_bytes()
    if not 0 < len(data) <= 256 * 1024**2 or b"<!DOCTYPE" in data:
        raise ValueError("bounded DTD-free gradient output required")
    root = ET.fromstring(data)
    if root.attrib.get("compressor") or root.attrib.get("byte_order") != "LittleEndian":
        raise ValueError(
            "expected explicit lossless uncompressed little-endian VTK output"
        )
    header = {"UInt32": ("<I", 4), "UInt64": ("<Q", 8)}[root.attrib["header_type"]]
    image = root.find("ImageData")
    pieces = image.findall("Piece")
    if len(pieces) != 1:
        raise ValueError("single image piece required")
    piece = pieces[0]
    extent = list(map(int, piece.attrib["Extent"].split()))
    points = math.prod(extent[n + 1] - extent[n] + 1 for n in (0, 2, 4))
    arrays = {}
    for field in piece.findall("PointData/DataArray"):
        if field.attrib["type"] != "Float64" or field.attrib["format"] != "binary":
            raise ValueError("inline binary Float64 point arrays required")
        encoded = "".join(field.text.split())
        count = ((header[1] + 2) // 3) * 4
        if encoded[:count].endswith("="):
            # VTK may encode the header and payload as separately padded blocks.
            raw = base64.b64decode(encoded[:count], validate=True) + base64.b64decode(
                encoded[count:], validate=True
            )
        else:
            raw = base64.b64decode(encoded, validate=True)
        size = struct.unpack(header[0], raw[: header[1]])[0]
        components = int(field.attrib.get("NumberOfComponents", "1"))
        if len(raw) - header[1] != size or size != points * components * 8:
            raise ValueError("gradient precision/association/extent mismatch")
        values = array.array("d")
        values.frombytes(raw[header[1] :])
        if sys.byteorder != "little":
            values.byteswap()
        if any(not math.isfinite(v) for v in values):
            raise ValueError("nonfinite gradient output")
        arrays[field.attrib["Name"]] = (components, values)
    return image.attrib, extent, arrays


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("runtime", "selection", "retained-fields", "output"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    args = parser.parse_args()
    resources = service_resources()
    runtime_path = args.runtime.resolve(strict=True)
    if not runtime_path.is_relative_to("/nix/store"):
        raise ValueError("exact immutable filter runtime required")
    runtime = json.loads(runtime_path.read_text())
    executable, bwrap = (
        Path(runtime[k]).resolve(strict=True) for k in ("filter", "bwrap")
    )
    if not all(
        p.is_relative_to("/nix/store") and p.is_file() for p in (executable, bwrap)
    ):
        raise ValueError("exact immutable native executable/bwrap required")
    selection = json.loads(args.selection.read_text())
    pci = selection["pci"]
    if (
        not re.fullmatch(r"[0-9a-f]{4}:[0-9a-f]{2}:[01][0-9a-f]\.[0-7]", pci)
        or selection["role"] != "compute"
        or selection["backend"] != "hip"
    ):
        raise ValueError("exact HIP compute selection required")
    # Use the established physical-card anchor and reject stranded/live durable
    # reservations. This probe has no job queue and does not replace the worker.
    anchor = Path(f"/run/user/{os.getuid()}/harbor-cad/cards/{pci}")
    fd = os.open(anchor, os.O_WRONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
    with os.fdopen(fd, "w") as lock:
        m = os.fstat(lock.fileno())
        if (
            not stat.S_ISREG(m.st_mode)
            or m.st_uid != os.getuid()
            or m.st_mode & 0o077
            or m.st_nlink != 1
        ):
            raise ValueError("owned private card anchor required")
        fcntl.flock(lock.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        journal = (
            Path(pwd.getpwuid(os.getuid()).pw_dir)
            / ".local/state/harbor-cad/admission/admission.sqlite3"
        )
        with sqlite3.connect(f"file:{journal}?mode=ro", uri=True) as database:
            if database.execute("SELECT count(*) FROM reservations").fetchone()[0]:
                raise ValueError(
                    "native reference requires the canonical journal to have no active reservations"
                )
        execute_probe(args, selection, runtime_path, executable, bwrap, resources)


def execute_probe(args, selection, runtime_path, executable, bwrap, resources):
    root = args.output.resolve()
    root.mkdir(parents=True, mode=0o700, exist_ok=False)
    inputs = root / "inputs"
    inputs.mkdir(mode=0o700)
    expected = {}
    for quadratic in (False, True):
        name = "quadratic" if quadratic else "linear"
        expected[name] = synthetic_image(inputs / f"{name}.vti", quadratic)
    for label in (
        "ghost",
        "float32",
        "cell_only",
        "multiple_piece",
        "rotated_direction",
    ):
        document = ET.parse(inputs / "linear.vti")
        piece = document.find("ImageData/Piece")
        velocity = piece.find("PointData/DataArray[@Name='physVelocity']")
        if label == "ghost":
            field = ET.SubElement(
                piece.find("PointData"),
                "DataArray",
                type="UInt8",
                Name="vtkGhostType",
                NumberOfComponents="1",
                format="binary",
            )
            field.text = (
                base64.b64encode(struct.pack("<I", 4096)).decode()
                + base64.b64encode(bytes(4096)).decode()
            )
        elif label == "float32":
            piece.find("PointData").remove(velocity)
            field = ET.SubElement(
                piece.find("PointData"),
                "DataArray",
                type="Float32",
                Name="physVelocity",
                NumberOfComponents="3",
                format="binary",
            )
            payload = bytes(4096 * 3 * 4)
            field.text = (
                base64.b64encode(struct.pack("<I", len(payload))).decode()
                + base64.b64encode(payload).decode()
            )
        elif label == "cell_only":
            piece.find("PointData").remove(velocity)
            field = ET.SubElement(
                piece.find("CellData"),
                "DataArray",
                type="Float64",
                Name="physVelocity",
                NumberOfComponents="3",
                format="binary",
            )
            payload = bytes(15**3 * 3 * 8)
            field.text = (
                base64.b64encode(struct.pack("<I", len(payload))).decode()
                + base64.b64encode(payload).decode()
            )
        elif label == "multiple_piece":
            ET.SubElement(
                document.find("ImageData"), "Piece", Extent=piece.attrib["Extent"]
            )
        else:
            document.find("ImageData").set("Direction", "0 -1 0 1 0 0 0 0 1")
        document.write(inputs / f"{label}.vti", encoding="utf-8", xml_declaration=True)
    retained_root = args.retained_fields.resolve(strict=True)
    snapshot_data = (retained_root / "snapshot.json").read_bytes()
    snapshot = json.loads(snapshot_data)
    shard = snapshot["times"][-1]["shards"]
    if len(shard) != 1:
        raise ValueError("one retained image shard required")
    source = retained_root / shard[0]
    manifest = next(f for f in snapshot["files"] if f["path"] == shard[0])
    data = source.read_bytes()
    if len(data) != manifest["bytes"] or digest(data) != manifest["sha256"]:
        raise ValueError("retained reference source mismatch")
    (inputs / "openlb.vti").write_bytes(data)
    results, rejections = [], []

    def invoke(label, request, cpu=False, environment=None):
        directory = root / label
        directory.mkdir(mode=0o700)
        (directory / "request.json").write_text(json.dumps(request, allow_nan=False))
        argv = [
            str(bwrap),
            "--unshare-all",
            "--die-with-parent",
            "--new-session",
            "--cap-drop",
            "ALL",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--tmpfs",
            "/tmp",
            "--ro-bind",
            "/nix/store",
            "/nix/store",
            "--ro-bind",
            str(inputs),
            "/inputs/fields",
            "--bind",
            str(directory),
            "/work",
            "--chdir",
            "/work",
        ]
        if not cpu:
            argv.extend(
                [
                    "--dev-bind",
                    f"/dev/dri/by-path/pci-{selection['pci']}-render",
                    "/dev/dri/renderD128",
                    "--dev-bind",
                    "/dev/kfd",
                    "/dev/kfd",
                    "--ro-bind",
                    "/sys",
                    "/sys",
                ]
            )
        argv.extend(
            [
                "--",
                str(executable),
                "gradient-cpu-reference" if cpu else "numerical_filter",
                "request.json",
            ]
        )
        start = time.monotonic()
        output = subprocess.run(
            argv,
            cwd=directory,
            env={"PATH": "/nonexistent", "OMP_NUM_THREADS": "1", **(environment or {})},
            capture_output=True,
            text=True,
            timeout=120,
            check=False,
        )
        (directory / "process.log").write_text(output.stdout + output.stderr)
        return directory, output, time.monotonic() - start

    def request(name, field="physVelocity", cpu=False):
        return {
            "schema_version": 1,
            "selection": None if cpu else selection,
            "field": field,
            "source_file": f"{name}.vti",
            "source_sha256": digest((inputs / f"{name}.vti").read_bytes()),
            "source_snapshot_sha256": digest(snapshot_data)
            if name == "openlb"
            else "a" * 64,
            "science_id": snapshot["science_id"] if name == "openlb" else "b" * 64,
            "execution_id": snapshot["execution_id"] if name == "openlb" else "c" * 64,
            "filter_execution_id": "d" * 64,
            "max_input_bytes": (inputs / f"{name}.vti").stat().st_size,
            "max_points": 1_000_000,
            "max_output_bytes": 256 * 1024**2,
        }

    for name, field in [
        ("linear", "physVelocity"),
        ("linear", "physPressure"),
        ("quadratic", "physVelocity"),
        ("quadratic", "physPressure"),
        ("openlb", "physVelocity"),
        ("openlb", "physPressure"),
    ]:
        compared = []
        receipts = []
        for cpu in (True, False):
            label = f"{name}-{field}-{'cpu' if cpu else 'hip'}"
            descriptor = request(name, field, cpu)
            directory, process, elapsed = invoke(label, descriptor, cpu)
            if process.returncode:
                raise RuntimeError(
                    f"{label} failed: {process.stderr}; inspect {directory / 'process.log'}"
                )
            receipt = json.loads(
                (directory / "numerical_filter-receipt.json").read_text()
            )
            if not cpu and (
                receipt["pci"] != selection["pci"]
                or receipt["backend_uuid"] != selection["backend_uuid"]
                or receipt["hip_dispatches"] <= 0
                or not receipt["gpu_kernel_completion_verified"]
                or receipt["software_fallback"]
                or receipt["adapter"] != "Viskores"
                or receipt["backend"] != "hip"
                or receipt["architecture"] != "gfx1100"
                or receipt["compiled_architecture"] != "gfx1100"
                or receipt["compiled_hip_version"] <= 0
                or receipt["compiled_hip_version"] != receipt["hip_runtime_version"]
                or receipt["compiled_hip_version"] != receipt["hip_driver_version"]
                or receipt["source_revision"]
                != "7c0494a68bff379d32d6b1fbaa3d10d27a73af54"
                or receipt["viskores_revision"]
                != "521f3b72aabe0bf37e9972975700df27adbbae71"
                or receipt["kokkos_revision"]
                != "6ecdf605e0f7639adec599d25cf0e206d7b8f9f5"
            ):
                raise ValueError(
                    "required native HIP execution/identity evidence missing"
                )
            metadata, extent, fields = read_gradient(directory / "gradient.vti")
            if name == "openlb":
                image, original_extent, _, original = read_vti(inputs / "openlb.vti")
                if (
                    extent != original_extent
                    or any(
                        list(map(float, metadata[k].split()))
                        != list(map(float, image.attrib[k].split()))
                        for k in ("Origin", "Spacing")
                    )
                    or any(fields[k] != values for k, values in original.items())
                ):
                    raise ValueError(
                        "original retained topology/coordinates/array values changed"
                    )
            else:
                original_metadata, original_extent, original = read_gradient(
                    inputs / f"{name}.vti"
                )
                if (
                    extent != original_extent
                    or any(
                        list(map(float, metadata[k].split()))
                        != list(map(float, original_metadata[k].split()))
                        for k in ("Origin", "Spacing")
                    )
                    or any(fields[k] != values for k, values in original.items())
                ):
                    raise ValueError(
                        "synthetic coordinates/arrays changed during filtering"
                    )
            gradient = fields["gradient"][1]
            components = 9 if field == "physVelocity" else 3
            serialized = (directory / "gradient.vti").read_bytes()
            if (
                fields["gradient"][0] != components
                or receipt["output_sha256"] != digest(serialized)
                or receipt["output_bytes"] != len(serialized)
                or receipt["precision"] != "float64"
                or receipt["association"] != "point"
                or receipt["gradient_ordering"]
                != (
                    "du/dx,du/dy,du/dz,dv/dx,dv/dy,dv/dz,dw/dx,dw/dy,dw/dz"
                    if field == "physVelocity"
                    else "dp/dx,dp/dy,dp/dz"
                )
                or any(
                    receipt[key] != descriptor[key]
                    for key in (
                        "science_id",
                        "execution_id",
                        "filter_execution_id",
                        "source_sha256",
                        "source_snapshot_sha256",
                    )
                )
            ):
                raise ValueError(
                    "receipt differs from source/precision/output identity"
                )
            if name in expected:
                maximum = max(
                    abs(gradient[components * index + c] - wanted[field][c])
                    for index, (interior, wanted) in enumerate(expected[name])
                    if interior
                    for c in range(components)
                )
                if maximum > 1e-10:
                    raise ValueError(
                        f"{label} analytical gradient maximum error {maximum} exceeds 1e-10"
                    )
                receipt["analytical_max_abs_error"] = maximum
                receipt["analytical_scope"] = (
                    "all points"
                    if name == "linear"
                    else "interior points; CPU/HIP comparison includes boundary"
                )
            compared.append(gradient)
            receipts.append({"receipt": receipt, "end_to_end_seconds": elapsed})
        disagreement = max(abs(a - b) for a, b in zip(*compared))
        if len(compared[0]) != len(compared[1]) or disagreement > 1e-10:
            raise ValueError(
                f"{name}/{field} CPU/HIP maximum disagreement {disagreement} exceeds 1e-10"
            )
        results.append(
            {
                "source": name,
                "field": field,
                "cpu_hip_max_abs_disagreement": disagreement,
                "runs": receipts,
            }
        )
    for label, mutation, environment, reason in [
        (
            "stale_uuid",
            {"selection": {**selection, "backend_uuid": "GPU-" + "0" * 32}},
            {},
            "HIP UUID/PCI mismatch",
        ),
        (
            "wrong_backend",
            {"selection": {**selection, "backend": "vulkan"}},
            {},
            "explicit HIP compute",
        ),
        (
            "missing_card",
            {"selection": {**selection, "pci": "ffff:ff:1f.7"}},
            {},
            "HIP PCI card missing",
        ),
        ("source_mutation", {"source_sha256": "0" * 64}, {}, "source bytes changed"),
        ("point_budget", {"max_points": 8}, {}, "image point budget"),
        ("unretained_path", {"source_file": "../linear.vti"}, {}, "path traversal"),
        (
            "architecture_spoof",
            {},
            {"HSA_OVERRIDE_GFX_VERSION": "11.0.0"},
            "architecture spoofing",
        ),
    ]:
        descriptor = {**request("linear"), **mutation}
        directory, process, _ = invoke(label, descriptor, environment=environment)
        if (
            not process.returncode
            or reason not in process.stderr
            or (directory / "gradient.vti").exists()
            or (directory / "numerical_filter-receipt.json").exists()
        ):
            raise ValueError(
                f"{label} did not reject before committed scientific output"
            )
        rejections.append(
            {
                "case": label,
                "exit_code": process.returncode,
                "reason": process.stderr.strip(),
            }
        )
    for label, reason in [
        ("ghost", "ghost arrays"),
        ("float32", "Float64 point field"),
        ("cell_only", "Float64 point field"),
        ("multiple_piece", "single full image piece"),
        ("rotated_direction", "axis-aligned identity image direction"),
    ]:
        directory, process, _ = invoke(f"reject-{label}", request(label))
        if (
            not process.returncode
            or reason not in process.stderr
            or (directory / "gradient.vti").exists()
            or (directory / "numerical_filter-receipt.json").exists()
        ):
            raise ValueError(
                f"{label} did not reject unsupported precision/association/topology"
            )
        rejections.append(
            {
                "case": label,
                "exit_code": process.returncode,
                "reason": process.stderr.strip(),
            }
        )
    directory, process, _ = invoke("reject-cpu-as-hip", request("linear"), cpu=True)
    if (
        not process.returncode
        or "CPU reference must be explicitly CPU-only" not in process.stderr
        or (directory / "gradient.vti").exists()
    ):
        raise ValueError("CPU reference accepted a required-HIP descriptor")
    rejections.append(
        {
            "case": "cpu_as_hip",
            "exit_code": process.returncode,
            "reason": process.stderr.strip(),
        }
    )
    report = {
        "schema_version": 1,
        "scope": "native bounded Float64 image gradient; synthetic/reference evidence only",
        "runtime": str(runtime_path),
        "runtime_sha256": digest(runtime_path.read_bytes()),
        "executable": str(executable),
        "executable_sha256": digest(executable.read_bytes()),
        "selection": selection,
        "results": results,
        "rejections": rejections,
        "service_resources_before": resources,
        "service_resources_after": service_resources(),
        "kfd_isolation": "unqualified by this probe; full sysfs visibility",
        "worker_lifecycle": "not exercised",
        "whole_card_vram_peak": "not measured",
        "instruction_trace": "not measured",
        "physical_validation": "unqualified",
    }
    (root / "verification.json").write_text(
        json.dumps(report, indent=2, allow_nan=False)
    )
    print(json.dumps(report, indent=2, allow_nan=False))


if __name__ == "__main__":
    main()
