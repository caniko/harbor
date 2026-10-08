"""Independent controlled-box receipt geometry and production-package refusal."""

import copy
import importlib.util
import os
import subprocess
from pathlib import Path

import pytest


def test_independent_variant_geometry_retains_world_origin_units_and_unchanged_gates(
    monkeypatch,
):
    repo = Path(__file__).parents[2]
    monkeypatch.syspath_prepend(str(repo / "scripts"))
    source = importlib.util.spec_from_file_location(
        "variant_campaign", repo / "scripts/verify_cad_variant_worker.py"
    )
    module = importlib.util.module_from_spec(source)
    source.loader.exec_module(module)
    transform = [
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
    ]
    spec = {
        "request": {
            "region_name": "solid",
            "dimensions": [
                {"value": 30.0, "unit": "mm"},
                {"value": 20.0, "unit": "mm"},
                {"value": 10.0, "unit": "mm"},
            ],
            "geometry_tolerance": {"value": 1e-6, "unit": "m"},
        },
        "source": {
            "geometry": {
                "synthetic": True,
                "bounds_m": [0.1, 0.12, -0.02, -0.01, 0.3, 0.31],
                "source_transform": transform,
            }
        },
    }
    regions = {
        "synthetic": True,
        "gap_healing": False,
        "geometry_tolerance": {"value": 1e-6, "unit": "m"},
        "regions": [
            {
                "name": "solid",
                "bounds_m": [0.1, 0.13, -0.02, 0.0, 0.3, 0.31],
                "volume_m3": 6e-6,
                "transform": transform,
                "triangles": 12,
                "source_unit": "mm",
                "stl_scale_to_m": 0.001,
            }
        ],
    }
    verified = module.verify_geometry(spec, regions)
    assert verified["bounds_m"] == [0.1, 0.13, -0.02, 0.0, 0.3, 0.31]
    assert (
        verified["volume_m3"] == pytest.approx(6e-6) and verified["placement_preserved"]
    )
    for key, value in [
        ("name", "foreign"),
        ("source_unit", "m"),
        ("volume_m3", 6.01e-6),
        ("bounds_m", [0.0, 0.03, 0.0, 0.02, 0.0, 0.01]),
        ("triangles", 0),
        (
            "transform",
            [
                1.0,
                0.0,
                0.0,
                0.0,
                0.0,
                1.0,
                0.0,
                0.0,
                0.0,
                0.0,
                1.0,
                0.0,
                0.0,
                0.0,
                0.0,
                1.0,
            ],
        ),
    ]:
        changed = copy.deepcopy(regions)
        changed["regions"][0][key] = value
        with pytest.raises(ValueError):
            module.verify_geometry(spec, changed)
    changed = copy.deepcopy(regions)
    changed["geometry_tolerance"]["value"] = 2e-6
    with pytest.raises(ValueError):
        module.verify_geometry(spec, changed)


def test_variant_native_qualifier_rejects_development_packages_before_outputs(tmp_path):
    local = tmp_path / "runtime.json"
    local.write_text("{}")
    output = tmp_path / "not-created"
    result = subprocess.run(
        [
            os.sys.executable,
            str(Path(__file__).parents[2] / "scripts/verify_cad_variant_worker.py"),
            "--executable",
            os.environ["HARBOR_CAD_TEST_BINARY"],
            "--mcp",
            str(local),
            "--runtime",
            str(local),
            "--fixture-runtime",
            str(local),
            "--authority",
            str(local),
            "--output",
            str(output),
        ],
        capture_output=True,
        text=True,
        check=False,
        timeout=10,
    )
    assert (
        result.returncode != 0
        and "exact immutable production package required" in result.stderr
    )
    assert not output.exists()
