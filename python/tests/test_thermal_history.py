"""Independent energy integrals and plane-wall eigenmode references."""

import importlib.util
import math
from pathlib import Path

import pytest


def module():
    path = Path(__file__).resolve().parents[2] / "adapters/thermal_history.py"
    spec = importlib.util.spec_from_file_location("thermal_history", path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def test_piecewise_linear_power_preserves_exact_energy_at_partial_intervals():
    bridge = module()
    history = [[0.0, 0.0], [10.0, 2.0], [20.0, 2.0]]
    assert bridge.history_value(history, 5.0) == 1.0
    assert bridge.history_energy(history, 5.0) == 2.5
    assert bridge.history_energy(history, 15.0) == 20.0
    with pytest.raises(ValueError):
        bridge.history_value(history, 25.0)


def test_robin_eigenvalues_and_decay_match_independent_plane_wall_reference():
    bridge = module()
    values = bridge.eigenvalues(1.0, 4)
    assert values[0] == pytest.approx(0.86033358901938, abs=2e-14)
    for index, root in enumerate(values):
        assert index * math.pi < root < index * math.pi + math.pi / 2
        assert root * math.tan(root) == pytest.approx(1.0, rel=1e-12)
    # Heisler first mode for Fo=1 at the centre: terms n>=1 are below 2e-6.
    expected = (
        4
        * math.sin(values[0])
        / (2 * values[0] + math.sin(2 * values[0]))
        * math.exp(-(values[0] ** 2))
    )
    observed = bridge.plane_wall_temperature(
        0.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        [[0.0, 0.0], [1.0, 0.0]],
        [[0.0, 0.0], [1.0, 0.0]],
    )
    assert observed == pytest.approx(expected, abs=2e-6)


def test_adiabatic_uniform_heating_and_constant_ambient_do_not_invent_heat_loss():
    bridge = module()
    # rho*c*V=2 J/K, integral of ramp through 5 s is 2.5 J.
    observed = bridge.plane_wall_temperature(
        0.3,
        5.0,
        1.0,
        1.0,
        2.0,
        0.0,
        250.0,
        [[0.0, 300.0], [10.0, 300.0]],
        [[0.0, 0.0], [10.0, 2.0]],
    )
    assert observed == 251.25
    # Stable exponential convolution of constant and linear forcing, also at tiny dt.
    for duration in (1e-10, 0.5, 10.0):
        integral = bridge.decay_integral(2.0, 3.0, 4.0, duration)
        constant = 3.0 * -math.expm1(-2.0 * duration) / 2.0
        ramp = 4.0 * (duration / 2.0 + math.expm1(-2.0 * duration) / 4.0)
        assert integral == pytest.approx(constant + ramp, rel=1e-12, abs=1e-20)


def fixture():
    return {
        "schema_version": 1,
        "synthetic": True,
        "backend": "cpu",
        "formulation": "plane_wall_robin",
        "size_m": [0.02, 0.01, 0.01],
        "resolution": 4,
        "geometry_tolerance_m": 1e-6,
        "initial_temperature_k": 293.15,
        "density_kg_m3": 7800.0,
        "specific_heat_j_kg_k": 500.0,
        "conductivity_w_m_k": 20.0,
        "material_temperature_domain_k": [240.0, 320.0],
        "convection_w_m2_k": 200.0,
        "duration_s": 120.0,
        "max_step_s": 1.0,
        "observation_times_s": [10.0, 60.0, 120.0],
        "ambient_history": [[0.0, 253.15], [60.0, 253.15], [120.0, 273.15]],
        "heater_history": [[0.0, 0.0], [60.0, 0.0], [120.0, 1.0]],
        "numerical_tolerance": 0.02,
        "energy_tolerance": 0.02,
        "geometry_provenance": "synthetic reference box",
        "material_provenance": "synthetic constant-property steel-like solid",
        "history_provenance": "prescribed synthetic cold soak then heater ramp",
        "convection_provenance": "prescribed synthetic h; no velocity conversion",
        "moisture_risk": {"assessment": "missing", "reason": "no humidity supplied"},
    }


def test_transient_admission_preserves_ranges_histories_and_unknown_moisture():
    bridge = module()
    request = fixture()
    bridge.validate(request)
    for field, value in [
        ("backend", "hip"),
        ("synthetic", False),
        ("convection_provenance", ""),
        ("density_kg_m3", 0.0),
        ("material_temperature_domain_k", [270.0, 320.0]),
        ("max_step_s", 1e-10),
        ("observation_times_s", [10.0, 120.0, 60.0]),
        ("numerical_tolerance", 0.1),
        ("heater_history", [[0.0, 0.0], [60.0, None], [120.0, 1.0]]),
        ("moisture_risk", None),
    ]:
        with pytest.raises((ValueError, TypeError)):
            bridge.validate({**request, field: value})
    with pytest.raises(ValueError):
        bridge.validate({**request, "implicit_velocity_to_h": True})


def test_convection_face_selection_retains_local_ccx_orientation_after_id_changes():
    bridge = module()
    spec = {**fixture(), "resolution": 1}
    ids = [8, 42, 9, 5, 76, 6, 1, 30]
    positions = [
        (0, 0, 0),
        (0.02, 0, 0),
        (0.02, 0.01, 0),
        (0, 0.01, 0),
        (0, 0, 0.01),
        (0.02, 0, 0.01),
        (0.02, 0.01, 0.01),
        (0, 0.01, 0.01),
    ]
    nodes = dict(zip(ids, positions, strict=True))
    faces = bridge.plane_wall_faces(spec, nodes, {93: ids})
    assert {f[1] for f in faces} == {4, 6}
    assert {tuple(sorted(f[2])) for f in faces} == {
        tuple(sorted([8, 5, 76, 30])),
        tuple(sorted([42, 9, 6, 1])),
    }
    with pytest.raises(ValueError):
        bridge.plane_wall_faces(spec, nodes, {93: ids, 94: ids})


def test_native_thermal_verification_rejects_incomplete_fields_and_false_energy():
    bridge = module()
    spec = {
        **fixture(),
        "resolution": 1,
        "duration_s": 2.0,
        "max_step_s": 1.0,
        "observation_times_s": [1.0, 2.0],
        "initial_temperature_k": 250.0,
        "convection_w_m2_k": 0.0,
        "density_kg_m3": 1000.0,
        "specific_heat_j_kg_k": 1000.0,
        "ambient_history": [[0.0, 250.0], [2.0, 250.0]],
        "heater_history": [[0.0, 1.0], [2.0, 1.0]],
    }
    positions = [
        (0, 0, 0),
        (0.02, 0, 0),
        (0.02, 0.01, 0),
        (0, 0.01, 0),
        (0, 0, 0.01),
        (0.02, 0, 0.01),
        (0.02, 0.01, 0.01),
        (0, 0.01, 0.01),
    ]
    nodes = dict(enumerate(positions, 1))
    cells = {53: list(nodes)}
    fields = {
        "temperature": [
            {"time": t, "values": {(n,): [250.0 + t / 2.0] for n in nodes}}
            for t in (1.0, 2.0)
        ]
    }
    checks, metrics, retained = bridge.verify(spec, nodes, cells, fields)
    assert checks["temperature"]["normalized_max_abs_error"] == 0.0
    assert checks["energy"]["maximum_relative_balance_error"] < 1e-12
    assert metrics[-1]["stored_energy_j"] == pytest.approx(2.0, rel=1e-12)
    assert [s["requested_s"] for s in retained] == [1.0, 2.0]
    fields["temperature"][0]["values"].pop((1,))
    with pytest.raises(ValueError):
        bridge.verify(spec, nodes, cells, fields)
    fields["temperature"][0]["values"] = {(n,): [250.0] for n in nodes}
    with pytest.raises(ValueError):
        bridge.verify(spec, nodes, cells, fields)
