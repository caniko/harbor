"""Independent CCX text-format and geometric selector rejection contracts."""

import importlib.util
from pathlib import Path

import pytest


def bridge():
    path = Path(__file__).resolve().parents[2] / "adapters/fem_reference.py"
    spec = importlib.util.spec_from_file_location("fem_reference", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_ccx_dat_preserves_ids_components_and_solver_step_parameters():
    parser = bridge().read_dat
    text = """
                        S T E P       1

                                INCREMENT     1

 temperatures for set NALL and time 0.1000000E+01

  7 2.931500E+02
  13 3.031500D+02

 heat flux (elem, integ.pnt.,qx,qy,qz) for set EALL and time 0.1000000E+01

  42 1 -1.000000E+04 0.000000E+00 0.000000E+00
  42 2 -1.000000E+04 0.000000E+00 0.000000E+00
"""
    fields = parser(text)
    assert fields["temperature"][0]["time"] == 1.0
    assert fields["temperature"][0]["values"] == {(7,): [293.15], (13,): [303.15]}
    assert fields["heat_flux"][0]["values"][(42, 2)] == [-10000.0, 0.0, 0.0]
    for broken in (
        text.replace("  13 ", "  7 "),
        text.replace("2.931500E+02", "NaN"),
        text.replace("0.1000000E+01", "Inf"),
        text.replace("  42 1 -1.000000E+04", "  42 1 2 -1.000000E+04"),
        text.replace("NALL", "UNRELATED"),
        text.replace("S T E P       1", "S T E P       0"),
        text.replace("INCREMENT     1", "UNKNOWN     1"),
    ):
        with pytest.raises(ValueError):
            parser(broken)


def test_semantic_face_selection_is_independent_of_face_numbering():
    module = bridge()
    bounds = [0.0, 0.02, 0.0, 0.01, 0.0, 0.03]
    surfaces = {
        98: [0, 0, 0, 0.01, 0, 0.03],
        7: [0.02, 0.02, 0, 0.01, 0, 0.03],
        64: [0, 0.02, 0, 0, 0, 0.03],
        11: [0, 0.02, 0.01, 0.01, 0, 0.03],
        37: [0, 0.02, 0, 0.01, 0, 0],
        25: [0, 0.02, 0, 0.01, 0.03, 0.03],
    }
    selected = module.classify_box_faces(surfaces, bounds, 1e-8)
    assert selected == {
        "xmin": 98,
        "xmax": 7,
        "ymin": 64,
        "ymax": 11,
        "zmin": 37,
        "zmax": 25,
    }
    with pytest.raises(ValueError):
        module.classify_box_faces({**surfaces, 100: surfaces[98]}, bounds, 1e-8)
    with pytest.raises(ValueError):
        module.classify_box_faces(
            {k: v for k, v in surfaces.items() if k != 37}, bounds, 1e-8
        )
    with pytest.raises(ValueError):
        module.classify_box_faces(
            {**surfaces, 98: [0, 0, 0, 0.005, 0, 0.03]}, bounds, 1e-8
        )


def test_native_inputs_reject_unsupported_material_units_backends_and_weakened_gates():
    module = bridge()
    spec = {
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
    module.validate(spec)
    for key, value in [
        ("schema_version", True),
        ("synthetic", False),
        ("backend", "hip"),
        ("resolution", 100000),
        ("size_m", [20, 0.01, 0.01]),
        ("size_m", [0.02, float("inf"), 0.01]),
        ("temperatures_k", [0, 293.15]),
        ("conductivity_w_m_k", None),
        ("conductivity_w_m_k", -1),
        ("numerical_tolerance", 0.1),
        ("conductivity_unit", "m/s"),
    ]:
        with pytest.raises(ValueError):
            module.validate({**spec, key: value})


def test_analytical_checks_reject_missing_samples_and_wrong_heat_flux_sign():
    module = bridge()
    spec = {
        "mode": "thermal_boundary",
        "size_m": [0.02, 0.01, 0.01],
        "temperatures_k": [293.15, 303.15],
        "numerical_tolerance": 1e-6,
        "conductivity_w_m_k": 20.0,
    }
    nodes = {7: [0, 0, 0], 13: [0.01, 0, 0], 19: [0.02, 0, 0]}
    cells = {42: []}
    fields = {
        "temperature": [
            {"time": 1, "values": {(7,): [293.15], (13,): [298.15], (19,): [303.15]}}
        ],
        "heat_flux": [
            {"time": 1, "values": {(42, ip): [-10000, 0, 0] for ip in range(1, 9)}}
        ],
    }
    assert all(
        check["passed"] for check in module.verify(spec, nodes, cells, fields).values()
    )
    fields["heat_flux"][0]["values"][(42, 4)] = [10000, 0, 0]
    with pytest.raises(ValueError, match="analytical gate failed"):
        module.verify(spec, nodes, cells, fields)
    del fields["heat_flux"][0]["values"][(42, 4)]
    with pytest.raises(ValueError, match="omit nodes/integration points"):
        module.verify(spec, nodes, cells, fields)


def test_ambiguous_duplicate_fields_and_nonstandard_json_numbers_are_rejected():
    module = bridge()
    assert module.strict_json(b'{"schema_version":1}') == {"schema_version": 1}
    for data in [
        b'{"conductivity_w_m_k":20,"conductivity_w_m_k":200}',
        b'{"conductivity_w_m_k":NaN}',
        b'{"conductivity_w_m_k":Infinity}',
    ]:
        with pytest.raises(ValueError):
            module.strict_json(data)
    with pytest.raises(TypeError):
        module.strict_json(b"[]")
