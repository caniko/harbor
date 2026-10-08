"""Fixed-mesh temporal convergence must not be hidden by spatial error cancellation."""

import copy
import importlib.util
from pathlib import Path

import pytest
from test_thermal_history import fixture


def module(monkeypatch):
    scripts = Path(__file__).resolve().parents[2] / "scripts"
    monkeypatch.syspath_prepend(str(scripts))
    source = importlib.util.spec_from_file_location(
        "thermal_gate", scripts / "verify_thermal_cpu.py"
    )
    gate = importlib.util.module_from_spec(source)
    source.loader.exec_module(gate)
    return gate


def evidence():
    requests = [{**fixture(), "max_step_s": dt} for dt in (2.0, 1.0, 0.5)]
    meshes = [{"nodes": {"7": [0.0, 0.0, 0.0]}} for _ in requests]
    fields = [
        {
            "association": "point",
            "unit": "K",
            "coordinate_unit": "m",
            "times": [
                {
                    "requested_s": stamp,
                    "observed_s": stamp,
                    "temperature_k": {"7": 300.019 - error},
                }
                for stamp in fixture()["observation_times_s"]
            ],
        }
        for error in (0.002, 0.001, 0.0005)
    ]
    return requests, fields, meshes


def test_temporal_refinement_is_distinct_from_fixed_spatial_error(monkeypatch):
    gate = module(monkeypatch)
    requests, fields, meshes = evidence()
    continuum_errors = [
        abs(field["times"][0]["temperature_k"]["7"] - 300.0) for field in fields
    ]
    assert continuum_errors[0] < continuum_errors[1] < continuum_errors[2] < 0.02
    report = gate.temporal_self_convergence(requests, fields, meshes)
    assert report["passed"] is True
    assert report["maximum_successive_differences_k"] == pytest.approx([0.001, 0.0005])
    assert report["observed_order"] == pytest.approx(1.0, abs=1e-9)
    assert report["native_steps_s"] == [2.0 / 64, 1.0 / 64, 0.5 / 64]


def test_refinement_cannot_replace_physics_change_units_or_mix_meshes(monkeypatch):
    gate = module(monkeypatch)
    requests, fields, meshes = evidence()
    for changes in (
        "physics",
        "gate",
        "mesh",
        "unit",
        "times",
        "nodes",
        "nonfinite",
        "divergence",
        "observed-times",
    ):
        req, fld, msh = copy.deepcopy((requests, fields, meshes))
        if changes == "physics":
            req[1]["convection_w_m2_k"] *= 2
        elif changes == "gate":
            req[1]["energy_tolerance"] *= 2
        elif changes == "mesh":
            msh[1]["nodes"]["7"][0] = 1.0
        elif changes == "unit":
            fld[1]["unit"] = "C"
        elif changes == "times":
            fld[1]["times"][0]["requested_s"] += 1.0
        elif changes == "nodes":
            fld[1]["times"][0]["temperature_k"] = {}
        elif changes == "nonfinite":
            fld[1]["times"][0]["temperature_k"]["7"] = float("nan")
        elif changes == "observed-times":
            fld[1]["times"][0]["observed_s"] += 0.01
        else:
            fld[2]["times"][0]["temperature_k"]["7"] += 0.02
        with pytest.raises(ValueError):
            gate.temporal_self_convergence(req, fld, msh)
