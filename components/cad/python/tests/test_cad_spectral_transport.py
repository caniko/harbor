"""Direct spectral transport must preserve original facets and optical budgets."""

import copy
import csv
import hashlib
import importlib.util
import json
import math
import struct
from pathlib import Path

import pytest


def bridge():
    path = Path(__file__).parents[2] / "adapters/cad_spectral_transport.py"
    spec = importlib.util.spec_from_file_location("cad_spectral_transport", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def box_stl(origin=(0.0, 0.0, 0.0)):
    # Twelve oriented native mm facets, original order deliberately unrelated
    # to geometric face order. No native library is needed for this fixture.
    vertices = [
        (x + origin[0], y + origin[1], z + origin[2])
        for x in (0, 1)
        for y in (0, 2)
        for z in (0, 3)
    ]
    faces = [
        (0, 1, 3),
        (0, 3, 2),
        (4, 6, 7),
        (4, 7, 5),
        (0, 4, 5),
        (0, 5, 1),
        (2, 3, 7),
        (2, 7, 6),
        (0, 2, 6),
        (0, 6, 4),
        (1, 5, 7),
        (1, 7, 3),
    ]
    data = bytearray(80) + struct.pack("<I", len(faces))
    for face in reversed(faces):
        points = [vertices[i] for i in face]
        a = [points[1][i] - points[0][i] for i in range(3)]
        b = [points[2][i] - points[0][i] for i in range(3)]
        n = [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
        length = math.sqrt(sum(v * v for v in n))
        data.extend(
            struct.pack(
                "<12fH", *[v / length for v in n], *[v for p in points for v in p], 0
            )
        )
    return bytes(data)


def test_native_stl_facets_keep_order_units_orientation_area_and_closed_topology():
    module = bridge()
    data = box_stl((10.0, 20.0, 30.0))
    facets = module.decode_stl(data, [0.01, 0.011, 0.02, 0.022, 0.03, 0.033], 1e-8)
    assert len(facets) == 12
    assert facets[0]["normal"] == [0.0, 0.0, 1.0]
    assert facets[0]["vertices_m"][0] == [0.01, 0.02, 0.033]
    assert math.fsum(f["area_m2"] for f in facets) == pytest.approx(22e-6, rel=1e-14)
    assert [f["index"] for f in facets] == list(range(12))
    for changed in (
        data[:-1],
        data + b"extra",
        data[:84] + data[134:],
        data[:84] + data[84:134] + data[84:134] + data[184:],
    ):
        with pytest.raises(ValueError):
            module.decode_stl(changed, [0.01, 0.011, 0.02, 0.022, 0.03, 0.033], 1e-8)
    changed = bytearray(data)
    struct.pack_into("<H", changed, 84 + 48, 1)
    with pytest.raises(ValueError):
        module.decode_stl(bytes(changed), [0.01, 0.011, 0.02, 0.022, 0.03, 0.033], 1e-8)


def test_geometric_visibility_checks_the_complete_closed_prism_without_invented_offset():
    module = bridge()
    box = [0.0, 0.001, 0.0, 0.002, 0.003, 0.006]
    assert module.ray_box([0.0005, 0.001, 0.0], [0.0, 0.0, 1.0], box)
    assert not module.ray_box([0.002, 0.001, 0.0], [0.0, 0.0, 1.0], box)
    assert not module.ray_box([0.0005, 0.001, 0.0], [0.0, 0.0, -1.0], box)
    assert module.ray_box(
        [0.002, 0.001, 0.002], [-math.sqrt(0.5), 0.0, math.sqrt(0.5)], box
    )


def transport_fixture(root):
    repo = Path(__file__).parents[2]
    request = json.loads((repo / "examples/cad-spectral-scene.json").read_text())
    request["geometry_tolerance"] = {"value": 1e-8, "unit": "m"}
    request["materials"][0]["ageing_action"] = {
        "availability": "known",
        "value": [0.2, 0.8],
        "provenance": "explicit manufactured action spectrum; no calibrated lifetime",
        "synthetic": True,
    }
    data = box_stl()
    (root / "solid.stl").write_bytes(data)
    spec = {
        "schema_version": 1,
        "synthetic": True,
        "backend": "cpu",
        "variant": "scalar_spectral",
        "precision": "Float32",
        "formulation": "opaque_lambertian_direct_only",
        "scene": {
            "schema_version": 1,
            "scene_id": "a" * 64,
            "request": request,
            "regions": [
                {
                    "assignment": request["assignments"][0],
                    "source": {
                        "geometry": {
                            "bounds_m": [0, 0.001, 0, 0.002, 0, 0.003],
                            "synthetic": True,
                            "geometry_tolerance_m": 1e-8,
                        }
                    },
                    "original_triangles": {
                        "path": "solid.stl",
                        "sha256": hashlib.sha256(data).hexdigest(),
                        "bytes": len(data),
                    },
                    "geometry": {"triangles": 12},
                }
            ],
            "missing_inputs": [],
            "transport_readiness": "prepared_not_executed",
            "ageing_readiness": "prepared_not_executed",
            "executed": False,
            "physical_validation": "unqualified",
            "limitations": [],
        },
        "source": {
            "kind": "directional",
            "propagation_direction": [0, 0, -1],
            "irradiance": [
                {"value": 1.0, "unit": "W/(m2*nm)"},
                {"value": 2e9, "unit": "W/(m2*m)"},
            ],
        },
        "source_provenance": "manufactured exact collimated spectral illumination; no atmosphere inference",
        "history": [
            {"time": {"value": 0, "unit": "s"}, "scale": 1.0},
            {"time": {"value": 1, "unit": "h"}, "scale": 3.0},
        ],
        "history_interpolation": "piecewise_linear_prescribed_scale",
        "history_provenance": "explicit synthetic source-amplitude history",
        "samples_per_triangle": 64,
        "seeds": [17, 29, 43],
        "relative_tolerance": 0.02,
        "maximum_geometry_rounding_error_m": 1e-9,
    }
    return spec


def test_direct_triangle_normalization_binds_originals_material_knots_units_and_area(
    tmp_path,
):
    module = bridge()
    spec = transport_fixture(tmp_path)
    normalized = module.normalize(spec, tmp_path)
    assert normalized["source_values_nm"] == [1.0, 2.0]
    assert normalized["history_integral_s"] == 7200.0
    assert normalized["regions"][0]["facets"][0]["area_m2"] == pytest.approx(1e-6)
    assert normalized["materials"]["synthetic_opaque"]["absorptivity"] == [0.8, 0.6]
    for field, value in (
        ("formulation", "all_bounce"),
        ("samples_per_triangle", 128.0),
        ("relative_tolerance", 0.03),
        ("maximum_geometry_rounding_error_m", 1e-6),
        ("execute", True),
    ):
        changed = {**spec, field: value}
        with pytest.raises(ValueError):
            module.normalize(changed, tmp_path)
    for mutate in (
        lambda value: value["scene"]["request"]["materials"][0].update(
            response={"availability": "missing", "reason": "unknown"}
        ),
        lambda value: value["scene"]["request"]["materials"][0]["response"][
            "value"
        ].update(reflectance=[0.2, 0.6]),
        lambda value: value["scene"]["regions"][0]["original_triangles"].update(
            path="../solid.stl"
        ),
        lambda value: value["scene"]["regions"][0]["original_triangles"].update(
            sha256="b" * 64
        ),
        lambda value: value["scene"]["regions"].append(value["scene"]["regions"][0]),
        lambda value: value["scene"].update(executed=True),
    ):
        changed = copy.deepcopy(spec)
        mutate(changed)
        with pytest.raises(ValueError):
            module.normalize(changed, tmp_path)


def original_packets(module, spec, normalized, path):
    conversions = []
    with path.open("x", newline="") as handle:
        writer = csv.writer(handle)
        writer.writerow(module.COLUMNS)
        for region in normalized["regions"]:
            for facet in region["facets"]:
                vertices = [
                    [struct.unpack("<f", struct.pack("<f", v))[0] for v in p]
                    for p in facet["vertices_m"]
                ]
                vector = module.cross(
                    module.subtract(vertices[1], vertices[0]),
                    module.subtract(vertices[2], vertices[0]),
                )
                area = math.sqrt(math.fsum(v * v for v in vector)) / 2.0
                conversions.append(
                    {
                        "region": region["name"],
                        "facet": facet["index"],
                        "original_area_m2": facet["area_m2"],
                        "native_area_m2": area,
                        "native_area_relative_error": abs(area / facet["area_m2"] - 1),
                        "maximum_vertex_rounding_error_m": max(
                            abs(a - b)
                            for p, q in zip(facet["vertices_m"], vertices)
                            for a, b in zip(p, q)
                        ),
                        "native_vertices_m": vertices,
                        "original_sha256": region["original"]["sha256"],
                    }
                )
                point = [math.fsum(p[i] for p in vertices) / 3 for i in range(3)]
                normal = facet["normal"]
                cosine = max(0.0, normal[2])
                pdf = 1.0 if cosine else 0.0
                for sample in range(spec["samples_per_triangle"]):
                    writer.writerow(
                        [
                            region["name"],
                            facet["index"],
                            sample,
                            0,
                            *point,
                            *normal,
                            0,
                            0,
                            1,
                            cosine,
                            pdf,
                            *[v * pdf for v in [1, 2, 2, 2]],
                            0.2,
                            0.4,
                            0.4,
                            0.4,
                        ]
                    )
    return conversions


def test_complete_original_packet_reconstruction_preserves_power_dose_and_opaque_energy(
    tmp_path,
):
    module = bridge()
    spec = transport_fixture(tmp_path)
    normalized = module.normalize(spec, tmp_path)
    path = tmp_path / "triangles.csv"
    conversions = original_packets(module, spec, normalized, path)
    facets = module.reconstruct(spec, normalized, conversions, path)
    assert len(facets) == 12
    assert sum(row["power_w"]["incident"] for row in facets) == pytest.approx(
        150 * 2e-6, rel=1e-12
    )
    for row in facets:
        assert row["channels_w_m2"]["absorbed"] + row["channels_w_m2"][
            "reflected_outgoing"
        ] == pytest.approx(row["channels_w_m2"]["incident"], rel=1e-12)
        assert row["dose_j_m2"]["incident"] == row["channels_w_m2"]["incident"] * 7200
    data = path.read_bytes()
    for changed in (
        data[:-100],
        data + data.split(b"\n", 2)[1] + b"\n",
        data.replace(b"solid,0,0,0", b"solid,1,0,0", 1),
        data.replace(b",0.2,0.4,0.4,0.4", b",0.8,0.4,0.4,0.4", 1),
    ):
        path.write_bytes(changed)
        with pytest.raises(ValueError):
            module.reconstruct(spec, normalized, conversions, path)
    path.write_bytes(data)
    for mutation in (
        lambda values: values.pop(),
        lambda values: values[0].update(original_sha256="0" * 64),
        lambda values: values[0]["native_vertices_m"][0].__setitem__(0, 1.0),
        lambda values: values[0].update(original_area_m2=1.0),
    ):
        changed = copy.deepcopy(conversions)
        mutation(changed)
        with pytest.raises(ValueError):
            module.reconstruct(spec, normalized, changed, path)
