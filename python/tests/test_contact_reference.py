"""Independent planar series-compliance, preload and thermal-opening controls."""

import copy
import importlib.util
from pathlib import Path

import pytest


def module():
    source = importlib.util.spec_from_file_location(
        "contact", Path(__file__).resolve().parents[2] / "adapters/contact_reference.py"
    )
    bridge = importlib.util.module_from_spec(source)
    source.loader.exec_module(bridge)
    return bridge


def request():
    return {
        "schema_version": 1,
        "synthetic": True,
        "backend": "cpu",
        "formulation": "planar_linear_penalty_contact",
        "size_m": [1e-3, 1e-3, 1e-3],
        "resolution": 2,
        "geometry_tolerance_m": 1e-8,
        "initial_gap_m": 0.0,
        "preload_compression_m": 0.5e-6,
        "final_compression_m": 1e-6,
        "young_modulus_pa": [1e8, 1e8],
        "expansion_per_k": [1e-5, 1e-5],
        "reference_temperature_k": 293.15,
        "final_temperatures_k": [293.15, 293.15],
        "contact_stiffness_pa_m": 1e12,
        "numerical_tolerance": 0.002,
        "material_provenance": "synthetic constant isotropic elasticity; Poisson ratio zero",
        "contact_provenance": "synthetic specified linear pressure/overclosure law",
        "boundary_provenance": "fixed transverse motion; bottom support and uniform top compression",
    }


def test_preload_and_temperature_preserve_series_compliance_and_opening():
    bridge, spec = module(), request()
    bridge.validate(spec)
    assert bridge.reference(spec, 1)["pressure_pa"] == pytest.approx(23809.5238095)
    assert bridge.reference(spec, 2)["pressure_pa"] == pytest.approx(47619.0476190)
    spec["final_temperatures_k"] = [273.15, 273.15]
    assert bridge.reference(spec, 2)["pressure_pa"] == pytest.approx(28571.4285714)
    spec["final_temperatures_k"] = [233.15, 233.15]
    cold = bridge.reference(spec, 2)
    assert cold["pressure_pa"] == 0
    assert cold["gap_m"] == pytest.approx(2e-7)
    assert cold["physical_time_s"] is None


def test_contact_rejects_unsupported_physics_and_weakened_acceptance():
    bridge, spec = module(), request()
    for key, value in (
        ("synthetic", False),
        ("backend", "hip"),
        ("formulation", "gasket_seal"),
        ("poisson_ratio", 0.3),
        ("initial_gap_m", -1e-6),
        ("contact_stiffness_pa_m", 0),
        ("young_modulus_pa", [1e8, float("nan")]),
        ("final_temperatures_k", [0, 293]),
        ("expansion_per_k", [1e-2, 1e-5]),
        ("final_compression_m", 1e-4),
        ("resolution", True),
        ("numerical_tolerance", 0.01),
        ("material_provenance", " "),
    ):
        with pytest.raises(ValueError):
            bridge.validate({**spec, key: value})


def test_native_coverage_force_balance_and_displacement_gate_are_independent():
    bridge, spec = module(), request()
    # Two complete n2 blocks. Use analytic nodal/element fields as independent
    # controls; this is a verifier test, not a substitute for a native solve.
    nodes, sets = (
        {},
        {"bottom": [], "top": [], "lower_interface": [], "upper_interface": []},
    )
    for block in range(2):
        for z in range(3):
            for y in range(3):
                for x in range(3):
                    tag = len(nodes) + 1
                    nodes[tag] = [x * 0.0005, y * 0.0005, z * 0.0005 + block * 0.001]
                    if z == 0:
                        sets["bottom" if block == 0 else "upper_interface"].append(tag)
                    if z == 2:
                        sets["lower_interface" if block == 0 else "top"].append(tag)
    cells = {tag: [] for tag in range(1, 17)}
    fields = {name: [] for name in ("displacement", "reaction_force", "stress")}
    for time, delta, pressure in (
        (1, 0.5e-6, 23809.52380952381),
        (2, 1e-6, 47619.04761904762),
    ):
        displacement, force, stress = {}, {}, {}
        for tag, xyz in nodes.items():
            dz = (
                -pressure / 1e8 * xyz[2]
                if tag <= 27
                else -delta + pressure / 1e8 * (0.002 - xyz[2])
            )
            displacement[tag,] = [0.0, 0.0, dz]
            fz = (
                pressure * 1e-6 / 9
                if tag in sets["bottom"]
                else (-pressure * 1e-6 / 9 if tag in sets["top"] else 0.0)
            )
            force[tag,] = [0.0, 0.0, fz]
        for tag in cells:
            for ip in range(1, 9):
                stress[tag, ip] = [0.0, 0.0, -pressure, 0.0, 0.0, 0.0]
        for name, values in zip(fields, (displacement, force, stress), strict=True):
            fields[name].append({"time": time, "values": values})
    checks = bridge.verify(spec, nodes, cells, sets, fields)
    assert len(checks) == 2 and all(v["passed"] for v in checks)
    mutations = []
    incomplete = copy.deepcopy(fields)
    incomplete["displacement"][0]["values"].pop((1,))
    mutations.append(incomplete)
    unbalanced = copy.deepcopy(fields)
    unbalanced["reaction_force"][1]["values"][sets["top"][0],][2] += 0.01
    mutations.append(unbalanced)
    moved = copy.deepcopy(fields)
    moved["displacement"][1]["values"][sets["lower_interface"][0],][2] += 1e-7
    mutations.append(moved)
    for changed in mutations:
        with pytest.raises(ValueError):
            bridge.verify(spec, nodes, cells, sets, changed)


def test_reaction_force_parser_is_opt_in_and_retains_native_component_identity():
    source = importlib.util.spec_from_file_location(
        "fem", Path(__file__).resolve().parents[2] / "adapters/fem_reference.py"
    )
    fem = importlib.util.module_from_spec(source)
    source.loader.exec_module(fem)
    data = "forces (fx,fy,fz) for set NALL and time 1.0\n1 0.0 0.0 -4.761905D-2\n"
    with pytest.raises(ValueError):
        fem.read_dat(data)
    values = fem.read_dat(data, reaction_forces=True)["reaction_force"][0]["values"]
    assert values == {(1,): [0.0, 0.0, -0.04761905]}
    for changed in (
        data + "1 0 0 0\n",
        data.replace("NALL", "TOP"),
        data.replace("D-2", "D+999"),
    ):
        with pytest.raises(ValueError):
            fem.read_dat(changed, reaction_forces=True)
