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


def test_ccx_dat_preserves_ids_components_and_physical_times():
    parser = bridge().read_dat
    text = """
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
