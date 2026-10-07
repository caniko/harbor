"""Independent original-field recheck of an executed thermal-to-contact bundle."""

import json
import math

from verify_contact_cpu import load


def verify(bundle, spec, job_id, source):
    fem = load("coupled_fem", source / "adapters/fem_reference.py")
    thermal = load("coupled_thermal", source / "adapters/thermal_history.py")
    projection = json.loads(
        (bundle / "stages/projection/projection-receipt.json").read_text()
    )
    assert projection["job_id"] == job_id and projection["process"] == "succeeded"
    assert projection["physical_validation"] == "unqualified"
    temperatures, checks = [], []
    for i, block in enumerate(("lower", "upper")):
        history = spec["thermal"][i]
        directory = bundle / f"stages/thermal-{block}"
        mesh = json.loads((directory / "mesh.json").read_text())
        nodes, cells = (
            {int(k): v for k, v in mesh[name].items()} for name in ("nodes", "elements")
        )
        raw = fem.read_dat((directory / "reference.dat").read_text())
        native_checks, _, retained = thermal.verify(history, nodes, cells, raw)
        receipt = json.loads((directory / "thermal-receipt.json").read_text())
        assert native_checks == receipt["numerical_verification"]
        selected = next(
            state
            for state in retained
            if state["requested_s"] == spec["coupling_time_s"]
        )
        values = {int(k): v for k, v in selected["temperature_k"].items()}
        nodal_capacitance = dict.fromkeys(nodes, 0.0)
        # Complete native positive C3D8 geometry is checked by the production
        # adapter and Rust verifier. Independently assemble rho*cp*V/8 weights.
        cell_capacity = (
            math.prod(history["size_m"])
            * history["density_kg_m3"]
            * history["specific_heat_j_kg_k"]
            / len(cells)
        )
        for cell in cells.values():
            assert len(cell) == 8 and len(set(cell)) == 8
            for node in cell:
                nodal_capacitance[node] += cell_capacity / 8
        capacity = math.fsum(nodal_capacitance.values())
        integral = math.fsum(nodal_capacitance[n] * values[n] for n in nodes)
        mean = integral / capacity
        loss = max(abs(v - mean) for v in values.values())
        p = projection["projections"][i]
        assert abs(p["destination_temperature_k"] - mean) <= 1e-10
        assert abs(p["capacitance_j_k"] / capacity - 1) <= 1e-12
        assert abs(p["maximum_abs_projection_error_k"] - loss) <= 1e-10
        assert loss <= spec["maximum_projection_error_k"][i]
        assert (
            p["transfer"]["relative_conservation_error"]
            <= spec["maximum_relative_conservation_error"]
        )
        assert abs(p["transfer"]["source_integral"] / integral - 1) <= 1e-12
        assert p["physical_time_s"] == spec["coupling_time_s"]
        assert p["native_time_s"] == selected["observed_s"]
        assert p["source_nodes"] == len(nodes)
        expected_origin = [0.0, 0.0, 0.0]
        if i:
            expected_origin[2] = (
                spec["mechanical"]["size_m"][2] + spec["mechanical"]["initial_gap_m"]
            )
        assert p["destination"]["origin_m"] == expected_origin
        assert p["destination"]["size_m"] == spec["mechanical"]["size_m"]
        assert p["destination"]["region"] == block
        assert len(p["surfaces"]) == 6
        for surface in p["surfaces"]:
            ids = mesh["boundary_node_sets"][surface["region"]]
            minimum = min(values[n] for n in ids)
            assert surface["minimum_surface_temperature_k"] == minimum
            assert surface["surface_nodes"] == len(ids)
            assert values[surface["minimum_node_id"]] == minimum
            assert surface["moisture_risk"]["status"] in {
                "missing_inputs",
                "inapplicable",
                "screening",
                "unsupported_screening",
            }
        temperatures.append(mean)
        checks.append(
            {
                "block": block,
                "native_thermal_checks": native_checks,
                "independent_temperature_k": mean,
                "independent_capacitance_j_k": capacity,
                "independent_maximum_projection_error_k": loss,
            }
        )
    contact = projection["derived_contact"]
    for key, value in spec["mechanical"].items():
        assert contact[key] == value
    assert all(
        abs(a - b) <= 1e-10
        for a, b in zip(contact["final_temperatures_k"], temperatures, strict=True)
    )
    return contact, checks
