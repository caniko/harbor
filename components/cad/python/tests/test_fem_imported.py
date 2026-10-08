"""Translated native references bind local analytic coordinates to approved CAD world planes."""

import importlib.util
from pathlib import Path

import pytest
from test_cad_mesh import fixture as cad_fixture
from test_cad_mesh import module as cad_bridge
from test_fem_reference import bridge


@pytest.mark.parametrize("mode", ["thermal_boundary", "free_expansion"])
def test_fem_reference_uses_explicit_origin_without_moving_imported_nodes(mode):
    fem = bridge()
    origin = [0.1, -0.02, 0.3]
    nodes = {7: origin[:], 13: [0.11, -0.015, 0.305], 19: [0.12, -0.01, 0.31]}
    spec = {
        "mode": mode,
        "size_m": [0.02, 0.01, 0.01],
        "temperatures_k": [293.15, 303.15],
        "numerical_tolerance": 1e-6,
        "conductivity_w_m_k": 20.0,
        "young_modulus_pa": 200e9,
        "poisson_ratio": 0.3,
        "expansion_per_k": 12e-6,
    }
    if mode == "thermal_boundary":
        fields = {
            "temperature": [
                {
                    "time": 1,
                    "values": {(7,): [293.15], (13,): [298.15], (19,): [303.15]},
                }
            ],
            "heat_flux": [
                {
                    "time": 1,
                    "values": {(42, ip): [-10000.0, 0.0, 0.0] for ip in range(1, 9)},
                }
            ],
        }
    else:
        fields = {
            "displacement": [
                {
                    "time": 1,
                    "values": {
                        (7,): [0.0, 0.0, 0.0],
                        (13,): [1.2e-6, 6e-7, 6e-7],
                        (19,): [2.4e-6, 1.2e-6, 1.2e-6],
                    },
                }
            ],
            "stress": [
                {"time": 1, "values": {(42, ip): [0.0] * 6 for ip in range(1, 9)}}
            ],
        }
    checks = fem.verify(spec, nodes, {42: []}, fields, origin=origin)
    assert all(
        v["passed"] and v["normalized_max_abs_error"] <= 1e-6 for v in checks.values()
    )
    assert nodes[7] == origin
    with pytest.raises(ValueError):
        fem.verify(spec, nodes, {42: []}, fields)
    with pytest.raises(ValueError):
        fem.verify(spec, nodes, {42: []}, fields, origin=[0.2, -0.02, 0.3])


def test_imported_fem_cannot_replace_bound_geometry_refinement_or_material_inputs():
    path = Path(__file__).resolve().parents[2] / "adapters/fem_imported.py"
    source = importlib.util.spec_from_file_location("fem_imported", path)
    imported = importlib.util.module_from_spec(source)
    source.loader.exec_module(imported)
    fem, cad = bridge(), cad_bridge()
    reference = {
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
    spec = {
        "schema_version": 1,
        "geometry": cad_fixture(),
        "reference": reference,
        "material_provenance": "prescribed synthetic constant-property material",
        "boundary_provenance": "prescribed analytical wall reference",
    }
    imported.validate(spec, fem, cad)
    for changed in [
        {**spec, "material_provenance": ""},
        {**spec, "geometry": {**spec["geometry"], "synthetic": False}},
        {**spec, "reference": {**reference, "size_m": [0.03, 0.01, 0.01]}},
        {**spec, "reference": {**reference, "resolution": 8}},
        {**spec, "reference": {**reference, "geometry_tolerance_m": 1e-7}},
        {**spec, "reference": {**reference, "numerical_tolerance": 0.01}},
        {**spec, "auto_contact": True},
    ]:
        with pytest.raises(ValueError):
            imported.validate(changed, fem, cad)
