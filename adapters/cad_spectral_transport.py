"""Bounded native direct-only spectral transport of original material-tagged STL."""

import csv
import hashlib
import importlib.metadata
import importlib.util
import json
import math
import os
import random
import re
import struct
import sys
import time
import uuid
from pathlib import Path

POLICY = "harbor-cad-cad-spectral-direct-cpu-v1"


def sibling(name):
    path = Path(__file__).with_name(name + ".py")
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def cross(a, b):
    return [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]


def subtract(a, b):
    return [x - y for x, y in zip(a, b)]


def decode_stl(data, bounds, tolerance):
    """Independently check the original closed box before any native conversion."""
    if len(data) < 84 or len(bounds) != 6 or not 1e-10 <= tolerance <= 1e-4:
        raise ValueError("complete bounded native box triangle input required")
    count = struct.unpack_from("<I", data, 80)[0]
    if not 12 <= count <= 192 or len(data) != 84 + 50 * count:
        raise ValueError("bounded complete original STL facet records required")
    if any(not math.isfinite(v) for v in bounds) or any(
        bounds[i] >= bounds[i + 1] for i in (0, 2, 4)
    ):
        raise ValueError("positive finite declared original box bounds required")
    facets, edges, unique, volumes = [], {}, set(), []
    for index in range(count):
        values = struct.unpack_from("<12fH", data, 84 + 50 * index)
        if values[-1] != 0 or any(not math.isfinite(v) for v in values[:-1]):
            raise ValueError(
                "finite original STL floats and zero colour/attributes required"
            )
        normal = list(values[:3])
        vertices = [tuple(values[3 + 3 * j : 6 + 3 * j]) for j in range(3)]
        key = tuple(sorted(vertices))
        if key in unique:
            raise ValueError("duplicate original facet rejected")
        unique.add(key)
        points = [[v * 0.001 for v in p] for p in vertices]
        vector = cross(subtract(points[1], points[0]), subtract(points[2], points[0]))
        length = math.sqrt(math.fsum(v * v for v in vector))
        if (
            length <= 0.0
            or abs(math.fsum(v * v for v in normal) - 1.0) > 1e-6
            or any(abs(v / length - n) > 1e-6 for v, n in zip(vector, normal))
        ):
            raise ValueError(
                "nondegenerate exact facet winding and original normals required"
            )
        faces = [
            face
            for face in range(6)
            if all(abs(p[face // 2] - bounds[face]) <= tolerance for p in points)
        ]
        if len(faces) != 1:
            raise ValueError("unambiguous original planar box face required")
        face = faces[0]
        expected = [0.0, 0.0, 0.0]
        expected[face // 2] = 1.0 if face % 2 else -1.0
        if any(abs(a - b) > 1e-6 for a, b in zip(expected, normal)):
            raise ValueError("outward box normals required; no orientation repair")
        for a, b in ((0, 1), (1, 2), (2, 0)):
            edge = tuple(sorted((vertices[a], vertices[b])))
            entry = edges.setdefault(edge, [0, 0])
            entry[0] += 1
            entry[1] += 1 if vertices[a] < vertices[b] else -1
        origin = bounds[::2]
        volumes.append(
            math.fsum(
                a * b
                for a, b in zip(
                    subtract(points[0], origin),
                    cross(subtract(points[1], origin), subtract(points[2], origin)),
                )
            )
            / 6.0
        )
        facets.append(
            {
                "index": index,
                "vertices_m": points,
                "normal": normal,
                "area_m2": length / 2.0,
                "boundary": face,
            }
        )
    if any(entry != [2, 0] for entry in edges.values()):
        raise ValueError("closed oriented manifold required; no welding or healing")
    lengths = [bounds[i + 1] - bounds[i] for i in (0, 2, 4)]
    for face in range(6):
        target = math.prod(lengths[axis] for axis in range(3) if axis != face // 2)
        area = math.fsum(f["area_m2"] for f in facets if f["boundary"] == face)
        if abs(area / target - 1.0) > 1e-10:
            raise ValueError("unchanged 1e-10 original surface-area gate required")
    if abs(math.fsum(volumes) / math.prod(lengths) - 1.0) > 1e-10:
        raise ValueError("unchanged 1e-10 original volume gate required")
    return facets


def ray_box(point, direction, bounds):
    """Independent analytical closed-prism visibility; no native epsilon offset."""
    lower, upper = -math.inf, math.inf
    for axis in range(3):
        if direction[axis] == 0.0:
            if not bounds[2 * axis] <= point[axis] <= bounds[2 * axis + 1]:
                return False
        else:
            a, b = [
                (bounds[2 * axis + side] - point[axis]) / direction[axis]
                for side in range(2)
            ]
            lower, upper = max(lower, min(a, b)), min(upper, max(a, b))
    return upper > max(lower, 0.0)


def normalize(spec, original_root):
    keys = {
        "schema_version",
        "synthetic",
        "backend",
        "variant",
        "precision",
        "formulation",
        "scene",
        "source",
        "source_provenance",
        "history",
        "history_interpolation",
        "history_provenance",
        "samples_per_triangle",
        "seeds",
        "relative_tolerance",
        "maximum_geometry_rounding_error_m",
    }
    if (
        set(spec) != keys
        or type(spec["schema_version"]) is not int
        or spec["schema_version"] != 1
        or type(spec["synthetic"]) is not bool
        or spec["backend"] != "cpu"
        or spec["variant"] != "scalar_spectral"
        or spec["precision"] != "Float32"
        or spec["formulation"] != "opaque_lambertian_direct_only"
    ):
        raise ValueError(
            "strict explicit CPU Float32 direct-only material-tagged triangle request required"
        )
    scene = spec["scene"]
    if (
        set(scene)
        != {
            "schema_version",
            "scene_id",
            "request",
            "regions",
            "missing_inputs",
            "transport_readiness",
            "ageing_readiness",
            "executed",
            "physical_validation",
            "limitations",
        }
        or type(scene["schema_version"]) is not int
        or scene["schema_version"] != 1
        or scene["executed"] is not False
        or scene["physical_validation"] != "unqualified"
        or not re.fullmatch("[0-9a-f]{64}", scene["scene_id"])
    ):
        raise ValueError(
            "complete immutable prepared scene identity required; preparation cannot claim transport"
        )
    request = scene["request"]
    if (
        set(request)
        != {
            "schema_version",
            "source_job",
            "wavelengths",
            "geometry_tolerance",
            "materials",
            "assignments",
        }
        or type(request["schema_version"]) is not int
        or request["schema_version"] != 1
    ):
        raise ValueError("strict original material-scene request required")
    uuid.UUID(request["source_job"])
    bridge = sibling("spectral_reference")
    # Reuse the locked source-unit, knot and temporal-quadrature contract. The
    # auxiliary reference-sensor normalisation is not transported geometry; each
    # original triangle separately supplies its native area/normal/visibility.
    optical = {
        "schema_version": 1,
        "synthetic": True,
        "backend": "cpu",
        "variant": "scalar_spectral",
        "precision": "Float32",
        "wavelengths": request["wavelengths"],
        "source": spec["source"],
        "source_provenance": spec["source_provenance"],
        "sensor_width": {"value": 1.0, "unit": "mm"},
        "sensor_height": {"value": 1.0, "unit": "mm"},
        "sensor_normal": [0, 0, 1],
        "occlusion": "none",
        "absorptivity": [1.0] * len(request["wavelengths"]),
        "optical_provenance": "unit numerical normalization; original region optical curves are independently bound",
        "ageing_action": [1.0] * len(request["wavelengths"]),
        "ageing_provenance": "unit numerical normalization; no material lifetime inference",
        "history": spec["history"],
        "history_interpolation": spec["history_interpolation"],
        "history_provenance": spec["history_provenance"],
        "samples": 1024,
        "seeds": spec["seeds"],
        "relative_tolerance": spec["relative_tolerance"],
    }
    normalized = bridge.normalize(optical)
    if spec["source"]["kind"] != "directional":
        raise ValueError(
            "collimated direct-only source required; no invented diffuse or interreflection contribution"
        )
    count = spec["samples_per_triangle"]
    rounding = bridge.number(spec["maximum_geometry_rounding_error_m"])
    if (
        type(count) is not int
        or not 64 <= count <= 4096
        or count & (count - 1)
        or not 0 < rounding <= 1e-8
    ):
        raise ValueError(
            "bounded exact per-facet samples and explicit original geometry-rounding limit required"
        )
    materials = {}
    missing = []
    n = len(normalized["wavelengths_nm"])
    if not 2 <= n <= 16:
        raise ValueError("bounded original CAD spectral knots required")
    scene_tolerance = bridge.quantity(
        request["geometry_tolerance"], {"m": 1.0, "mm": 0.001, "nm": 1e-9}
    )
    if not 1e-10 <= scene_tolerance <= 1e-4:
        raise ValueError("bounded unchanged original scene geometry tolerance required")
    if not 1 <= len(request["materials"]) <= 16:
        raise ValueError("bounded original region material definitions required")
    for material in request["materials"]:
        if (
            set(material) != {"name", "response", "ageing_action"}
            or not re.fullmatch("[A-Za-z0-9_-]{1,64}", material["name"])
            or material["name"] in materials
        ):
            raise ValueError(
                "unambiguous complete original material identities required"
            )
        response = material["response"]
        if (
            set(response) != {"availability", "value", "provenance", "synthetic"}
            or response["availability"] != "known"
            or type(response["synthetic"]) is not bool
            or not isinstance(response["provenance"], str)
            or not response["provenance"].strip()
            or len(response["provenance"]) > 4096
        ):
            raise ValueError(
                "explicit known spectral response required; missing optical data cannot execute"
            )
        value = response["value"]
        if (
            set(value)
            != {"formulation", "interpolation", "reflectance", "absorptivity"}
            or value["formulation"] != "opaque_lambertian"
            or value["interpolation"] != "piecewise_linear"
            or any(
                len(value[key]) != n
                or any(not 0 <= bridge.number(v) <= 1 for v in value[key])
                for key in ("reflectance", "absorptivity")
            )
            or any(
                abs(r + a - 1) > 1e-12
                for r, a in zip(value["reflectance"], value["absorptivity"])
            )
        ):
            raise ValueError(
                "unchanged original opaque energy-closure and complete spectral curves required"
            )
        action = material["ageing_action"]
        if action.get("availability") == "known":
            if (
                set(action) != {"availability", "value", "provenance", "synthetic"}
                or type(action["synthetic"]) is not bool
                or not isinstance(action["provenance"], str)
                or not action["provenance"].strip()
                or len(action["provenance"]) > 4096
                or len(action["value"]) != n
                or any(not 0 <= bridge.number(v) <= 1 for v in action["value"])
            ):
                raise ValueError(
                    "complete explicit bounded ageing response and provenance required"
                )
            ageing = action["value"]
        elif (
            set(action) == {"availability", "reason"}
            and action["availability"] == "missing"
            and isinstance(action["reason"], str)
            and action["reason"].strip()
            and len(action["reason"]) <= 4096
        ):
            ageing = None
            missing.append(material["name"] + ".ageing_action")
        else:
            raise ValueError("explicit known or missing ageing response required")
        materials[material["name"]] = {**value, "ageing_action": ageing}
    if (
        scene["missing_inputs"] != missing
        or scene["transport_readiness"] != "prepared_not_executed"
        or scene["ageing_readiness"]
        != ("missing_inputs" if missing else "prepared_not_executed")
    ):
        raise ValueError(
            "unchanged preparation readiness and explicit missing inputs required"
        )
    assignments = {}
    for entry in request["assignments"]:
        if (
            set(entry) != {"region_name", "material_name", "provenance"}
            or not re.fullmatch("[A-Za-z0-9_-]{1,64}", entry["region_name"])
            or entry["region_name"] in assignments
            or entry["material_name"] not in materials
            or not isinstance(entry["provenance"], str)
            or not entry["provenance"].strip()
            or len(entry["provenance"]) > 4096
        ):
            raise ValueError(
                "one explicit complete known material assignment per original region required"
            )
        assignments[entry["region_name"]] = entry
    if (
        not 1 <= len(assignments) <= 16
        or {entry["material_name"] for entry in assignments.values()}
        != materials.keys()
        or len(scene["regions"]) != len(assignments)
    ):
        raise ValueError("bounded complete original material-region coverage required")
    regions, seen = [], set()
    for region in scene["regions"]:
        if set(region) != {"assignment", "source", "original_triangles", "geometry"}:
            raise ValueError("exact original scene region envelope required")
        name = region["assignment"]["region_name"]
        if name in seen or region["assignment"] != assignments.get(name):
            raise ValueError("unchanged unique original region assignment required")
        seen.add(name)
        record = region["original_triangles"]
        path = Path(original_root) / record["path"]
        if (
            record["path"] != name + ".stl"
            or path.is_symlink()
            or not path.is_file()
            or type(record["bytes"]) is not int
            or not 684 <= record["bytes"] <= 84 + 50 * 192
            or path.stat().st_size != record["bytes"]
        ):
            raise ValueError(
                "bounded closed original STL with exact region-local path required"
            )
        data = path.read_bytes()
        if hashlib.sha256(data).hexdigest() != record["sha256"]:
            raise ValueError(
                "original CAD triangle bytes differ from approved identity"
            )
        geometry = region["source"]["geometry"]
        tolerance = bridge.number(geometry["geometry_tolerance_m"])
        if (
            geometry["synthetic"] != spec["synthetic"]
            or type(geometry["synthetic"]) is not bool
            or rounding > tolerance
            or rounding > scene_tolerance
            or scene_tolerance > tolerance
        ):
            raise ValueError(
                "unchanged source provenance and nonweakened geometry-rounding gate required"
            )
        facets = decode_stl(data, geometry["bounds_m"], scene_tolerance)
        if len(facets) != region["geometry"]["triangles"]:
            raise ValueError("complete original facet count required")
        regions.append(
            {
                "name": name,
                "material": region["assignment"]["material_name"],
                "bounds_m": geometry["bounds_m"],
                "facets": facets,
                "original": record,
            }
        )
    for i, a in enumerate(regions):
        for b in regions[i + 1 :]:
            if not any(
                a["bounds_m"][2 * axis + 1] < b["bounds_m"][2 * axis]
                or b["bounds_m"][2 * axis + 1] < a["bounds_m"][2 * axis]
                for axis in range(3)
            ):
                raise ValueError(
                    "disjoint separated opaque boxes required; no touching or overlapping material inference"
                )
    if math.fsum(len(r["facets"]) for r in regions) * count > 65536:
        raise ValueError("bounded aggregate complete facet-sample allowance exhausted")
    return {
        "wavelengths_nm": normalized["wavelengths_nm"],
        "source_values_nm": normalized["source_values_nm"],
        "history_integral_s": normalized["history_integral_s"],
        "regions": regions,
        "materials": materials,
    }


def native_scene(spec, normalized, mi):
    bridge = sibling("spectral_reference")
    definition = {
        "type": "scene",
        "source": {
            "type": "directional",
            "direction": spec["source"]["propagation_direction"],
            "irradiance": bridge.native_spectrum(
                normalized["wavelengths_nm"], normalized["source_values_nm"]
            ),
        },
    }
    shapes, conversions = {}, []
    for region in normalized["regions"]:
        response = normalized["materials"][region["material"]]
        bsdf = mi.load_dict(
            {
                "type": "diffuse",
                "reflectance": bridge.native_spectrum(
                    normalized["wavelengths_nm"], response["reflectance"]
                ),
            }
        )
        for facet in region["facets"]:
            name = region["name"] + "-facet-" + str(facet["index"])
            props = mi.Properties()
            props["face_normals"] = True
            props["bsdf"] = bsdf
            mesh = mi.Mesh(
                name, 3, 1, props, has_vertex_normals=False, has_vertex_texcoords=False
            )
            mesh.set_id(name)
            params = mi.traverse(mesh)
            original = [v for p in facet["vertices_m"] for v in p]
            params["vertex_positions"] = original
            params["faces"] = [0, 1, 2]
            params.update()
            converted = [float(v) for v in params["vertex_positions"]]
            rounding = max(abs(a - b) for a, b in zip(original, converted))
            area = float(mesh.surface_area())
            area_error = abs(area / facet["area_m2"] - 1.0)
            if (
                not math.isfinite(area)
                or area <= 0
                or rounding > spec["maximum_geometry_rounding_error_m"]
                or area_error > 1e-6
                or [int(v) for v in params["faces"]] != [0, 1, 2]
                or mesh.has_vertex_normals()
            ):
                raise ValueError(
                    "bounded explicit native Float32 geometry conversion and unchanged outward facet topology required"
                )
            conversions.append(
                {
                    "region": region["name"],
                    "facet": facet["index"],
                    "original_area_m2": facet["area_m2"],
                    "native_area_m2": area,
                    "native_area_relative_error": area_error,
                    "maximum_vertex_rounding_error_m": rounding,
                    "native_vertices_m": [converted[i : i + 3] for i in (0, 3, 6)],
                    "original_sha256": region["original"]["sha256"],
                }
            )
            shapes[(region["name"], facet["index"])] = mesh
            definition[name] = mesh
    return mi.load_dict(definition, parallel=False, optimize=False), shapes, conversions


COLUMNS = [
    "region",
    "facet",
    "sample",
    "knot_offset",
    "x_m",
    "y_m",
    "z_m",
    "normal_x",
    "normal_y",
    "normal_z",
    "towards_source_x",
    "towards_source_y",
    "towards_source_z",
    "native_cosine",
    "native_pdf",
    *[f"native_weight_w_m2_nm_{i}" for i in range(4)],
    *[f"native_reflectance_{i}" for i in range(4)],
]


def measure(spec, normalized, scene, shapes, seed, path, mi, dr):
    rng = random.Random(seed)
    wavelengths = normalized["wavelengths_nm"]
    with path.open("x", newline="") as handle:
        writer = csv.writer(handle)
        writer.writerow(COLUMNS)
        for region in normalized["regions"]:
            for facet in region["facets"]:
                shape = shapes[(region["name"], facet["index"])]
                for sample in range(spec["samples_per_triangle"]):
                    position = shape.sample_position(0.0, [rng.random(), rng.random()])
                    emitter_sample = [rng.random(), rng.random()]
                    for offset in range(0, len(wavelengths), 4):
                        subset = wavelengths[offset : offset + 4]
                        si = mi.SurfaceInteraction3f()
                        si.p = position.p
                        si.n = position.n
                        si.sh_frame = mi.Frame3f(position.n)
                        si.wi = mi.Vector3f(0, 0, 1)
                        si.wavelengths = mi.Spectrum(
                            subset + [subset[-1]] * (4 - len(subset))
                        )
                        direction, weights = scene.sample_emitter_direction(
                            si, emitter_sample, True
                        )
                        cosine = max(0.0, float(dr.dot(position.n, direction.d)))
                        reflectance = (
                            shape.bsdf().eval(
                                mi.BSDFContext(), si, mi.Vector3f(0, 0, 1)
                            )
                            * dr.pi
                        )
                        numbers = [
                            *position.p,
                            *position.n,
                            *direction.d,
                            cosine,
                            float(direction.pdf),
                            *weights,
                            *reflectance,
                        ]
                        if any(not math.isfinite(float(v)) for v in numbers):
                            raise ValueError(
                                "finite original native facet positions, directions and optical observations required"
                            )
                        writer.writerow(
                            [
                                region["name"],
                                facet["index"],
                                sample,
                                offset,
                                *map(float, numbers),
                            ]
                        )


def point_on_triangle(point, vertices, tolerance):
    a, b, p = (
        subtract(vertices[1], vertices[0]),
        subtract(vertices[2], vertices[0]),
        subtract(point, vertices[0]),
    )
    dot = lambda x, y: math.fsum(v * w for v, w in zip(x, y))
    aa, ab, bb, pa, pb = dot(a, a), dot(a, b), dot(b, b), dot(p, a), dot(p, b)
    denominator = aa * bb - ab * ab
    if denominator <= 0:
        return False
    u, v = (bb * pa - ab * pb) / denominator, (aa * pb - ab * pa) / denominator
    projected = [vertices[0][i] + u * a[i] + v * b[i] for i in range(3)]
    return (
        u >= -2e-6
        and v >= -2e-6
        and u + v <= 1 + 2e-6
        and max(abs(a - b) for a, b in zip(projected, point)) <= tolerance
    )


def verify_conversions(spec, normalized, conversions):
    expected = [(r, f) for r in normalized["regions"] for f in r["facets"]]
    if len(conversions) != len(expected):
        raise ValueError(
            "exact complete original/native geometry conversion coverage required"
        )
    for row, (region, facet) in zip(conversions, expected):
        vertices = [
            [struct.unpack("<f", struct.pack("<f", v))[0] for v in p]
            for p in facet["vertices_m"]
        ]
        error = max(
            abs(a - b)
            for original, native in zip(facet["vertices_m"], vertices)
            for a, b in zip(original, native)
        )
        vector = cross(
            subtract(vertices[1], vertices[0]), subtract(vertices[2], vertices[0])
        )
        geometric_area = math.sqrt(math.fsum(v * v for v in vector)) / 2.0
        if (
            set(row)
            != {
                "region",
                "facet",
                "original_area_m2",
                "native_area_m2",
                "native_area_relative_error",
                "maximum_vertex_rounding_error_m",
                "native_vertices_m",
                "original_sha256",
            }
            or row["region"] != region["name"]
            or row["facet"] != facet["index"]
            or row["original_area_m2"] != facet["area_m2"]
            or row["original_sha256"] != region["original"]["sha256"]
            or row["native_vertices_m"] != vertices
            or row["maximum_vertex_rounding_error_m"] != error
            or error > spec["maximum_geometry_rounding_error_m"]
            or not math.isfinite(row["native_area_m2"])
            or row["native_area_m2"] <= 0.0
            or not math.isclose(
                row["native_area_m2"], geometric_area, rel_tol=1e-6, abs_tol=0.0
            )
            or row["native_area_relative_error"]
            != abs(row["native_area_m2"] / facet["area_m2"] - 1.0)
            or row["native_area_relative_error"] > 1e-6
        ):
            raise ValueError(
                "exact independently reconstructed original Float32 to SI to native Float32 conversion required"
            )


def reconstruct(spec, normalized, conversions, path):
    """Independently check every original packet and reduce exact original areas."""
    bridge = sibling("spectral_reference")
    wavelengths = normalized["wavelengths_nm"]
    n = len(wavelengths)
    verify_conversions(spec, normalized, conversions)
    records = {(row["region"], row["facet"]): row for row in conversions}
    results = []
    with path.open(newline="") as handle:
        reader = csv.DictReader(handle)
        if reader.fieldnames != COLUMNS:
            raise ValueError(
                "exact complete original native facet packet header required"
            )
        for region in normalized["regions"]:
            material = normalized["materials"][region["material"]]
            optical = {
                "incident": [1.0] * n,
                "absorbed": material["absorptivity"],
                "reflected_outgoing": material["reflectance"],
            }
            if material["ageing_action"] is not None:
                optical["ageing"] = material["ageing_action"]
            for facet in region["facets"]:
                identity = region["name"], facet["index"]
                conversion = records[identity]
                sums, references = [[] for _ in range(n)], [[] for _ in range(n)]
                for sample in range(spec["samples_per_triangle"]):
                    point = None
                    for offset in range(0, n, 4):
                        row = next(reader, None)
                        if (
                            row is None
                            or None in row
                            or any(value is None for value in row.values())
                            or row["region"] != region["name"]
                            or row["facet"] != str(facet["index"])
                            or row["sample"] != str(sample)
                            or row["knot_offset"] != str(offset)
                        ):
                            raise ValueError(
                                "complete ordered original native facet/sample/knot coverage required"
                            )
                        values = [bridge.number(float(row[key])) for key in COLUMNS[4:]]
                        position, normal, direction = (
                            values[:3],
                            values[3:6],
                            values[6:9],
                        )
                        cosine, pdf, weights, reflectance = (
                            values[9],
                            values[10],
                            values[11:15],
                            values[15:19],
                        )
                        if point is not None and point != position:
                            raise ValueError(
                                "wavelength packets must preserve the same original surface point"
                            )
                        point = position
                        if (
                            not point_on_triangle(
                                position,
                                conversion["native_vertices_m"],
                                spec["maximum_geometry_rounding_error_m"],
                            )
                            or any(
                                abs(a - b) > 2e-6
                                for a, b in zip(normal, facet["normal"])
                            )
                            or any(
                                abs(a + b) > 2e-6
                                for a, b in zip(
                                    direction, spec["source"]["propagation_direction"]
                                )
                            )
                        ):
                            raise ValueError(
                                "exact original facet association, position, normal and collimated native direction required"
                            )
                        expected_cosine = max(
                            0.0,
                            -math.fsum(
                                a * b
                                for a, b in zip(
                                    facet["normal"],
                                    spec["source"]["propagation_direction"],
                                )
                            ),
                        )
                        visible = not any(
                            ray_box(position, direction, other["bounds_m"])
                            for other in normalized["regions"]
                            if other["name"] != region["name"]
                        )
                        expected = 1.0 if visible and expected_cosine > 0 else 0.0
                        # On inward/back-facing rays, the complete native receiver
                        # box occludes itself. Both native PDF possibilities are
                        # acceptable only when the exact cosine is identically zero.
                        if (
                            abs(cosine - expected_cosine) > 2e-6
                            or pdf not in (0.0, 1.0)
                            or (expected_cosine > 0 and pdf != expected)
                        ):
                            raise ValueError(
                                "independent analytical original-box visibility and cosine gate failed"
                            )
                        for i in range(4):
                            knot = min(offset + i, n - 1)
                            source = normalized["source_values_nm"][knot]
                            expected_weight = source if pdf == 1 else 0.0
                            if not math.isclose(
                                weights[i], expected_weight, rel_tol=2e-6, abs_tol=0.0
                            ) or not math.isclose(
                                reflectance[i],
                                material["reflectance"][knot],
                                rel_tol=2e-6,
                                abs_tol=1e-8,
                            ):
                                raise ValueError(
                                    "unchanged original native source spectrum and material BSDF response required"
                                )
                            if offset + i < n:
                                sums[knot].append(weights[i] * cosine)
                                references[knot].append(
                                    source * expected_cosine * expected
                                )
                means = [math.fsum(v) / spec["samples_per_triangle"] for v in sums]
                analytic = [
                    math.fsum(v) / spec["samples_per_triangle"] for v in references
                ]
                channels = {
                    name: bridge.product_integral(wavelengths, means, weight)
                    for name, weight in optical.items()
                }
                reference = {
                    name: bridge.product_integral(wavelengths, analytic, weight)
                    for name, weight in optical.items()
                }
                errors = {
                    name: abs(channels[name] / value - 1.0)
                    if value
                    else (0.0 if channels[name] == 0.0 else math.inf)
                    for name, value in reference.items()
                }
                if any(v > spec["relative_tolerance"] for v in errors.values()):
                    raise ValueError(
                        "unchanged independent 0.02 original-point direct transport gate exceeded"
                    )
                results.append(
                    {
                        "region": region["name"],
                        "material": region["material"],
                        "facet": facet["index"],
                        "geometric_boundary": facet["boundary"],
                        "original_area_m2": facet["area_m2"],
                        "samples": spec["samples_per_triangle"],
                        "mean_spectral_irradiance_w_m2_nm": means,
                        "channels_w_m2": channels,
                        "reference_channels_w_m2": reference,
                        "relative_errors": errors,
                        "power_w": {
                            name: value * facet["area_m2"]
                            for name, value in channels.items()
                        },
                        "dose_j_m2": {
                            name: value * normalized["history_integral_s"]
                            for name, value in channels.items()
                        },
                        "energy_j": {
                            name: value
                            * normalized["history_integral_s"]
                            * facet["area_m2"]
                            for name, value in channels.items()
                        },
                        "ageing_status": "missing_inputs"
                        if material["ageing_action"] is None
                        else "prescribed_action_no_lifetime_calibration",
                    }
                )
        if next(reader, None) is not None:
            raise ValueError("extra original native packets rejected")
    return results


def run(spec, normalized, mi, dr):
    scene, shapes, conversions = native_scene(spec, normalized, mi)
    observations = []
    for seed in spec["seeds"]:
        path = Path(f"triangles-{seed}.csv")
        measure(spec, normalized, scene, shapes, seed, path, mi, dr)
        facets = reconstruct(spec, normalized, conversions, path)
        observations.append(
            {
                "seed": seed,
                "original": {
                    "path": path.name,
                    "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                    "bytes": path.stat().st_size,
                },
                "facets": facets,
            }
        )
    return {
        "schema_version": 1,
        "adapter": "Mitsuba",
        "versions": {
            "mitsuba": importlib.metadata.version("mitsuba"),
            "drjit": importlib.metadata.version("drjit"),
        },
        "backend": "cpu",
        "variant": mi.variant(),
        "precision": "Float32",
        "reduction_precision": "Float64_compensated",
        "formulation": spec["formulation"],
        "executed": True,
        "software_fallback": False,
        "input": spec,
        "geometry_conversions": conversions,
        "observations": observations,
        "history_integral_s": normalized["history_integral_s"],
        "numerical_verification": "independent exact original-point analytical box visibility, spectral products and complete packet reconstruction",
        "sampling_convergence": "not_assessed",
        "interreflection": "excluded_by_explicit_direct_only_model",
        "physical_validation": "unqualified",
    }


def main():
    if len(sys.argv) != 3 or sys.argv[1] != "reference":
        raise ValueError("usage: harbor-cad-cad-spectral-direct reference REQUEST.json")
    foundation = sibling("fem_reference")
    request = Path(sys.argv[2])
    raw = foundation.read_regular(request, 1024 * 1024)
    spec = foundation.strict_json(raw)
    if os.environ.get("HARBOR_CAD_CAD_SPECTRAL_POLICY") != POLICY:
        raise ValueError("exact operation-only native CAD spectral sandbox required")
    sandbox = foundation.cpu_sandbox(
        "HARBOR_CAD_CAD_SPECTRAL_POLICY",
        POLICY,
        "/spectral-runtime-closure.txt",
        request,
    )
    if os.statvfs("/source").f_flag & os.ST_RDONLY == 0:
        raise ValueError("read-only original CAD triangle source mount required")
    sandbox["checks"]["original_source_readonly"] = True
    normalized = normalize(spec, Path("/source"))
    if any(
        path.name != "cad-spectral.log" or path.is_symlink() or not path.is_file()
        for path in Path.cwd().iterdir()
    ):
        raise ValueError(
            "new closed stage directory with at most the owned native capture log required"
        )
    import drjit as dr
    import mitsuba as mi

    if {name: importlib.metadata.version(name) for name in ("mitsuba", "drjit")} != {
        "mitsuba": "3.9.1",
        "drjit": "1.5.0",
    }:
        raise ValueError("exact isolated compatible Mitsuba/Dr.Jit ABI required")
    mi.set_variant("scalar_spectral")
    start = time.monotonic()
    receipt = run(spec, normalized, mi, dr)
    receipt.update(
        request_sha256=hashlib.sha256(raw).hexdigest(),
        sandbox=sandbox,
        elapsed_s=time.monotonic() - start,
    )
    with Path("cad-spectral-receipt.json").open("x") as handle:
        json.dump(receipt, handle, indent=2, allow_nan=False)
    print(
        json.dumps(
            {
                "executed": True,
                "observations": len(receipt["observations"]),
                "physical_validation": "unqualified",
            }
        )
    )


if __name__ == "__main__":
    main()
