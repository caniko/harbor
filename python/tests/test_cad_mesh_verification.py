"""Independent Cartesian mesh checks catch incorrect geometry and correspondence."""

import copy
import importlib.util
import itertools
from pathlib import Path

import pytest
from test_cad_mesh import fixture


def module(monkeypatch):
    scripts = Path(__file__).resolve().parents[2] / "scripts"
    monkeypatch.syspath_prepend(str(scripts))
    source = importlib.util.spec_from_file_location(
        "cad_gate", scripts / "verify_cad_mesh.py"
    )
    gate = importlib.util.module_from_spec(source)
    source.loader.exec_module(gate)
    return gate


def evidence():
    spec = {**fixture(), "resolution": 2}
    n = spec["resolution"]
    bounds = spec["bounds_m"]
    ids = {
        ijk: tag for tag, ijk in enumerate(itertools.product(range(n + 1), repeat=3), 1)
    }
    nodes = {
        str(tag): [
            bounds[2 * i] + ijk[i] * (bounds[2 * i + 1] - bounds[2 * i]) / n
            for i in range(3)
        ]
        for ijk, tag in ids.items()
    }
    offsets = [
        (0, 0, 0),
        (1, 0, 0),
        (1, 1, 0),
        (0, 1, 0),
        (0, 0, 1),
        (1, 0, 1),
        (1, 1, 1),
        (0, 1, 1),
    ]
    cells = {
        str(tag): [ids[tuple(origin[i] + off[i] for i in range(3))] for off in offsets]
        for tag, origin in enumerate(itertools.product(range(n), repeat=3), 1)
    }
    sets = {
        axis + side: [
            tag for ijk, tag in ids.items() if ijk[i] == (0 if side == "min" else n)
        ]
        for i, axis in enumerate("xyz")
        for side in ("min", "max")
    }
    return spec, {
        "coordinate_unit": "m",
        "element_type": "C3D8",
        "synthetic": True,
        "positive_gauss_jacobians": True,
        "nodes": nodes,
        "elements": cells,
        "boundary_node_sets": sets,
        "integrated_volume_m3": spec["volume_m3"],
    }


def test_cartesian_reference_retains_nonzero_world_placement(monkeypatch):
    report = module(monkeypatch).verify_box_mesh(*evidence())
    assert (
        report["passed"] is True and report["nodes"] == 27 and report["elements"] == 8
    )
    assert report["oriented_volume_m3"] == pytest.approx(2e-6)
    assert report["relative_volume_error"] < 1e-10


def test_independent_correspondence_rejects_changed_bounds_orientation_and_faces(
    monkeypatch,
):
    gate = module(monkeypatch)
    for case in (
        "placement",
        "orientation",
        "overlap",
        "face",
        "units",
        "volume",
        "provenance",
    ):
        spec, mesh = copy.deepcopy(evidence())
        if case == "placement":
            mesh["nodes"]["1"][0] += 1.0
        elif case == "orientation":
            mesh["elements"]["1"][1], mesh["elements"]["1"][3] = (
                mesh["elements"]["1"][3],
                mesh["elements"]["1"][1],
            )
        elif case == "overlap":
            mesh["elements"]["2"] = mesh["elements"]["1"]
        elif case == "face":
            mesh["boundary_node_sets"]["xmin"] = mesh["boundary_node_sets"]["xmax"]
        elif case == "units":
            mesh["coordinate_unit"] = "mm"
        elif case == "volume":
            mesh["integrated_volume_m3"] *= 2
        else:
            mesh["synthetic"] = False
        with pytest.raises(ValueError):
            gate.verify_box_mesh(spec, mesh)
