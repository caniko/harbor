"""CAD/OCC correspondence input checks preserve source placement and units."""

import hashlib
import importlib.util
from pathlib import Path
from types import SimpleNamespace

import pytest


def module():
    path = Path(__file__).resolve().parents[2] / "adapters/cad_mesh.py"
    source = importlib.util.spec_from_file_location("cad_mesh", path)
    bridge = importlib.util.module_from_spec(source)
    source.loader.exec_module(bridge)
    return bridge


def fixture():
    return {
        "schema_version": 1,
        "synthetic": True,
        "backend": "cpu",
        "formulation": "imported_axis_aligned_box",
        "geometry_provenance": "controlled synthetic FreeCAD solid exported by the patched sandbox importer",
        "brep_file": "solid.brep",
        "brep_sha256": hashlib.sha256(b"brep fixture").hexdigest(),
        "brep_bytes": 12,
        "region_name": "solid",
        "bounds_m": [0.1, 0.12, -0.02, -0.01, 0.3, 0.31],
        "volume_m3": 2e-6,
        "source_unit": "mm",
        "scale_to_m": 0.001,
        "placement_translation_unit": "mm",
        "source_transform": [
            1.0,
            0.0,
            0.0,
            100.0,
            0.0,
            1.0,
            0.0,
            -20.0,
            0.0,
            0.0,
            1.0,
            300.0,
            0.0,
            0.0,
            0.0,
            1.0,
        ],
        "resolution": 4,
        "geometry_tolerance_m": 1e-6,
        "volume_relative_tolerance": 1e-10,
    }


def test_imported_mesh_descriptor_retains_world_bounds_placement_and_source_identity():
    bridge = module()
    request = fixture()
    original = request.copy()
    bridge.validate(request)
    assert request == original
    assert bridge.expected_lengths(request) == pytest.approx([0.02, 0.01, 0.01])
    assert bridge.expected_volume(request) == pytest.approx(2e-6)


def test_mesh_correspondence_rejects_ambiguous_geometry_implicit_units_or_weak_gates():
    bridge = module()
    for key, value in [
        ("source_unit", "m"),
        ("scale_to_m", 1.0),
        ("placement_translation_unit", "m"),
        ("brep_file", "../solid.brep"),
        ("brep_sha256", "g" * 64),
        ("volume_m3", 3e-6),
        ("bounds_m", [0.0, 0.0, 0.0, 0.01, 0.0, 0.01]),
        ("resolution", True),
        ("resolution", 33),
        ("geometry_tolerance_m", 0.01),
        ("volume_relative_tolerance", 0.001),
        ("geometry_provenance", ""),
        ("backend", "hip"),
        ("auto_heal_gaps", True),
        ("brep_bytes", True),
        (
            "source_transform",
            [
                2.0,
                0.0,
                0.0,
                100.0,
                0.0,
                1.0,
                0.0,
                -20.0,
                0.0,
                0.0,
                1.0,
                300.0,
                0.0,
                0.0,
                0.0,
                1.0,
            ],
        ),
    ]:
        with pytest.raises((ValueError, TypeError)):
            bridge.validate({**fixture(), key: value})


def test_brep_import_scales_before_occ_binding_without_healing_or_generic_dilation():
    path = Path(__file__).resolve().parents[2] / "adapters/fem_reference.py"
    source = importlib.util.spec_from_file_location("fem_brep", path)
    fem = importlib.util.module_from_spec(source)
    source.loader.exec_module(fem)
    options, calls = {}, []

    def import_shapes(path, **kwargs):
        assert options["Geometry.OCCScaling"] == 0.001
        assert all(
            options["Geometry.OCC" + name] == 0
            for name in (
                "FixDegenerated",
                "FixSmallEdges",
                "FixSmallFaces",
                "SewFaces",
                "MakeSolids",
            )
        )
        calls.append((path, kwargs))
        return [(3, 19)]

    gmsh = SimpleNamespace(
        option=SimpleNamespace(setNumber=options.__setitem__),
        model=SimpleNamespace(occ=SimpleNamespace(importShapes=import_shapes)),
    )
    assert fem.import_brep(gmsh, fixture()) == 19
    assert calls == [("/inputs/solid.brep", {"highestDimOnly": True, "format": "brep"})]
    for unsupported in ([], [(2, 19)], [(3, 19), (3, 20)]):
        gmsh.model.occ.importShapes = lambda *args, result=unsupported, **kwargs: result
        with pytest.raises(ValueError, match="exactly one closed solid"):
            fem.import_brep(gmsh, fixture())
