"""Analytical spherical-cap contours and SI conservation gates, independent of OpenLB."""

import importlib.util
import math
from pathlib import Path

import pytest


def module():
    spec = importlib.util.spec_from_file_location(
        "wetting", Path(__file__).resolve().parents[2] / "adapters/wetting_reference.py"
    )
    bridge = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(bridge)
    return bridge


def request(angle=90):
    return {
        "schema_version": 1,
        "synthetic": True,
        "backend": "cpu",
        "formulation": "well_balanced_contact_angle_2d",
        "diameter_m": 48e-6,
        "resolution": 48,
        "interface_width_m": 6e-6,
        "density_liquid_kg_m3": 1000.0,
        "density_vapor_kg_m3": 1000.0,
        "viscosity_liquid_m2_s": 1e-6,
        "viscosity_vapor_m2_s": 1e-6,
        "surface_tension_n_m": 1e-4,
        "contact_angle_deg": angle,
        "phase_relaxation_time": 1.0,
        "steps": 16000,
        "observation_steps": [0, 8000, 16000],
        "mass_tolerance": 1e-3,
        "angle_tolerance_deg": 5.0,
        "material_provenance": "synthetic equal-property reference fluid",
        "boundary_provenance": "explicit uniform planar wall angle",
    }


def cap(spec, initial=False):
    n = spec["resolution"]
    dx = spec["diameter_m"] / n
    cx, radius = 1.25 * n, n / 2
    cy = (
        1.0
        if initial
        else 0.5 - radius * math.cos(math.radians(spec["contact_angle_deg"]))
    )
    width = spec["interface_width_m"] / dx
    rows = ["x_m,y_m,material,phi,u_lattice,v_lattice"]
    for y in range(int(1.5 * n) + 1):
        for x in range(int(2.5 * n) + 1):
            phi = (1 + math.tanh(4 * (math.hypot(x - cx, y - cy) - radius) / width)) / 2
            rows.append(
                f"{x * dx:.17g},{y * dx:.17g},{2 if y in (0, int(1.5 * n)) else 1},{phi:.17g},0,0"
            )
    return ("\n".join(rows) + "\n").encode()


@pytest.mark.parametrize("angle", [60, 90, 120])
def test_independent_whole_contour_fit_recovers_wall_contact_angle(angle):
    bridge, spec = module(), request(angle)
    spec["resolution"] = 96
    observed = bridge.assess_field(spec, cap(spec))
    assert observed["contact_angle_deg"] == pytest.approx(angle, abs=0.12)
    assert observed["circle_radius_m"] == pytest.approx(
        spec["diameter_m"] / 2, rel=0.001
    )
    assert observed["relative_radial_residual"] < 0.003
    assert observed["contour_points"] > 20


def test_native_field_rejects_missing_duplicate_nonfinite_or_changed_wall_cells():
    bridge, spec = module(), request()
    lines = cap(spec).decode().splitlines(keepends=True)
    for changed in (
        lines[:-1],
        [*lines, lines[1]],
        [lines[0], lines[1].replace(",0,0\n", ",nan,0\n"), *lines[2:]],
        [lines[0], lines[1].replace(",2,", ",1,"), *lines[2:]],
    ):
        with pytest.raises(ValueError):
            bridge.assess_field(spec, "".join(changed).encode())


def test_explicit_si_mapping_and_nonpromotion_of_real_water_air_ratio():
    bridge, spec = module(), request()
    units = bridge.validate(spec)
    assert units["spacing_m"] == pytest.approx(1e-6)
    assert units["physical_step_s"] == pytest.approx(1 / 6 * 1e-6)
    assert units["surface_tension_lattice"] == pytest.approx(1e-4 / 0.036)
    for key, value in (
        ("density_vapor_kg_m3", 1.0),
        ("synthetic", False),
        ("resolution", True),
        ("interface_width_m", 1e-7),
        ("mass_tolerance", 0.1),
        ("angle_tolerance_deg", 6),
        ("surface_tension_n_m", 0.072),
        ("boundary_provenance", " "),
        ("steps", float("inf")),
    ):
        with pytest.raises(ValueError):
            bridge.validate({**spec, key: value})


def test_phase_mass_and_angle_failures_do_not_change_explicit_acceptance():
    bridge, spec = module(), request()
    check = bridge.assess_field(spec, cap(spec))
    snapshots = [
        {"step": step, "check": dict(check)} for step in spec["observation_steps"]
    ]
    assert bridge.verify(spec, snapshots)["mass_passed"]
    snapshots[-1]["check"]["droplet_area_m2"] *= 1.002
    snapshots[-1]["check"]["contact_angle_deg"] += 6
    result = bridge.verify(spec, snapshots)
    assert result["mass_relative_error"] == pytest.approx(0.002)
    assert not result["mass_passed"] and not result["angle_passed"]


def test_exact_native_initial_cap_tangency_is_one_circle_not_a_second_droplet():
    bridge, spec = module(), request()
    spec["resolution"] = 36
    data = cap(spec, initial=True)
    grid, _ = bridge.read_field(data, spec["diameter_m"] / 36)
    assert grid[45, 19][1] == 0.5
    observed = bridge.assess_field(spec, data)
    assert observed["contact_angle_deg"] == pytest.approx(
        math.degrees(math.acos(-0.5 / 18)), abs=0.12
    )


def test_long_equilibrium_integration_requires_a_bounded_explicit_step_budget():
    bridge, spec = module(), request()
    spec["steps"] = 160000
    spec["observation_steps"] = [0, 80000, 120000, 160000]
    assert bridge.validate(spec)["physical_step_s"] * spec["steps"] == pytest.approx(
        0.02666666666666667
    )
    spec["steps"] = 200001
    spec["observation_steps"][-1] = spec["steps"]
    with pytest.raises(ValueError):
        bridge.validate(spec)
