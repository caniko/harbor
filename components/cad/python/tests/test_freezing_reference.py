"""Manufactured Stefan fields and corrupted native observations, without a solver."""

import csv
import hashlib
import importlib.util
import io
import json
import math
import os
import subprocess
import xml.etree.ElementTree as ET
from pathlib import Path

import pytest


def module(name):
    path = Path(__file__).resolve().parents[2] / f"adapters/{name}.py"
    spec = importlib.util.spec_from_file_location(name, path)
    instance = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(instance)
    return instance


def request():
    spec = json.loads(
        (
            Path(__file__).resolve().parents[2] / "examples/freezing-reference.json"
        ).read_text()
    )
    return {**spec, "resolution": 32, "steps": 1024, "observation_steps": [0, 1024]}


def manufactured(root):
    """Known similarity solution; zero-thickness Dirichlet nodes have no mass."""
    spec = request()
    bridge, fem = module("freezing_reference"), module("fem_reference")
    dx, dt, stefan = bridge.validate(spec, fem)
    n, ny = spec["resolution"], spec["resolution"] // 8
    mass_cell = spec["density_kg_m3"] * dx * dx * spec["size_m"][2]
    mass = mass_cell * (n - 1) * ny
    scale = spec["specific_heat_j_kg_k"] * 10
    snapshots = []
    for step in spec["observation_steps"]:
        rows = [
            [
                "i",
                "j",
                "x_m",
                "y_m",
                "material",
                "specific_enthalpy_j_kg",
                "temperature_k",
                "liquid_fraction",
            ]
        ]
        energy, liquid = [], []
        # Independent root value for Ste=0.1, rather than the verifier's bisection.
        parameter = 0.22001627274293786
        front = 2 * parameter * math.sqrt(1 / 6)
        for j in range(ny):
            for i in range(n):
                theta = (
                    0
                    if i == 0
                    else 1
                    if step == 0 or i / n >= front
                    else math.erf((i / n) / (2 * math.sqrt(1 / 6)))
                    / math.erf(parameter)
                )
                phase = float(i != 0 and (step == 0 or i / n >= front))
                h = scale * theta + spec["latent_heat_j_kg"] * phase
                rows.append(
                    [
                        i,
                        j,
                        i * dx,
                        (j + 0.5) * dx,
                        3 if i == 0 else 1,
                        h,
                        263.15 + theta * 10,
                        phase,
                    ]
                )
                if i:
                    energy.append(mass_cell * h)
                    liquid.append(mass_cell * phase)
        path = f"freezing-{step}.csv"
        with (root / path).open("w") as stream:
            csv.writer(stream).writerows(rows)
        energy = math.fsum(energy)
        snapshots.append(
            {
                "path": path,
                "step": step,
                "time_s": step * dt,
                "energy_j": energy,
                "mass_kg": mass,
                "liquid_mass_kg": math.fsum(liquid),
                "cold_exchange_j": energy - mass * 110000,
                "reflecting_exchange_j": 0.0,
            }
        )
    with (root / "heat-exchange.csv").open("w") as stream:
        writer = csv.writer(stream)
        writer.writerow(["step", "time_s", "cold_exchange_j", "reflecting_exchange_j"])
        for step in range(1, spec["steps"] + 1):
            writer.writerow(
                [
                    step,
                    step * dt,
                    snapshots[-1]["cold_exchange_j"] if step == spec["steps"] else 0.0,
                    0.0,
                ]
            )
    receipt = {
        "schema_version": 1,
        "adapter": "OpenLB",
        "backend": "cpu",
        "executed": True,
        "software_fallback": False,
        "precision": "float64",
        "source_revision": "145cd54810b468f4b6fd3ed86b10644264841578",
        "formulation": spec["formulation"],
        "dimensionality": 2,
        "synthetic": True,
        "request": spec,
        "shape": [n + 1, ny],
        "spacing_m": dx,
        "physical_step_s": dt,
        "stefan_number": stefan,
        "cell_mass_kg": mass_cell,
        "active_volume_m3": (n - 1) * ny * dx * dx * spec["size_m"][2],
        "active_control_bounds_m": [
            [0.5 * dx, spec["size_m"][0] - 0.5 * dx],
            [0, spec["size_m"][1]],
            [0, spec["size_m"][2]],
        ],
        "snapshots": snapshots,
        "energy_zero": "solid at prescribed cold wall temperature",
        "physical_validation": "unqualified",
    }
    return spec, receipt, bridge, fem


def test_manufactured_stefan_fields_conserve_active_mass_energy_and_known_similarity(
    tmp_path,
):
    spec, receipt, bridge, fem = manufactured(tmp_path)
    checks, observations, hashes = bridge.verify(spec, receipt, tmp_path, fem)
    assert bridge.similarity_parameter(0.1) == pytest.approx(
        0.22001627274293786, abs=1e-15
    )
    assert all(c["passed"] for c in checks.values())
    assert checks["temperature"]["error"] < 1e-12
    assert checks["mass"]["error"] < 1e-12
    assert checks["energy"]["error"] < 1e-12
    assert observations[-1]["solid_mass_kg"] > 0
    assert set(hashes) == {"freezing-0.csv", "freezing-1024.csv", "heat-exchange.csv"}


@pytest.mark.parametrize(
    "mutation",
    [
        "coverage",
        "duplicate",
        "geometry",
        "phase",
        "temperature",
        "flux",
        "time",
        "scope",
        "symlink",
    ],
)
def test_original_stefan_grid_and_ledger_reject_self_consistent_missing_or_changed_fields(
    tmp_path, mutation
):
    spec, receipt, bridge, fem = manufactured(tmp_path)
    path = tmp_path / "freezing-1024.csv"
    rows = list(csv.reader(io.StringIO(path.read_text())))
    if mutation == "coverage":
        rows.pop()
    elif mutation == "duplicate":
        rows.append(rows[1])
    elif mutation == "geometry":
        rows[2][2] = str(float(rows[2][2]) + 1e-6)
    elif mutation == "phase":
        rows[2][7] = "0.5"
    elif mutation == "temperature":
        rows[2][6] = str(float(rows[2][6]) + 1)
        rows[2][5] = str(float(rows[2][5]) + spec["specific_heat_j_kg_k"])
    elif mutation == "flux":
        ledger = tmp_path / "heat-exchange.csv"
        entries = ledger.read_text().splitlines(keepends=True)
        ledger.write_text("".join(entries[:-1]))
    elif mutation == "time":
        receipt["snapshots"][-1]["time_s"] += 1e-5
    elif mutation == "scope":
        receipt["executed"] = 1
    elif mutation == "symlink":
        path.rename(tmp_path / "redirect.csv")
        path.symlink_to(tmp_path / "redirect.csv")
    if mutation in {"coverage", "duplicate", "geometry", "phase", "temperature"}:
        with path.open("w") as stream:
            csv.writer(stream).writerows(rows)
    with pytest.raises((ValueError, OSError)):
        bridge.verify(spec, receipt, tmp_path, fem)


def test_rust_python_freezing_validation_preserves_scale_and_rejects_integer_physics_drift():
    bridge, fem = module("freezing_reference"), module("fem_reference")
    spec = request()
    binary = os.environ["HARBOR_CAD_TEST_BINARY"]
    dx, dt, stefan = bridge.validate(spec, fem)
    inspected = json.loads(
        subprocess.check_output(
            [binary, "case", "validate-freezing-reference", "/dev/stdin"],
            input=json.dumps(spec).encode(),
        )
    )
    assert inspected["scale"]["spacing_m"] == dx
    assert inspected["scale"]["physical_step_s"] == pytest.approx(dt, rel=1e-15)
    assert inspected["scale"]["stefan_number"] == stefan
    for key, value in (
        ("resolution", 2**32 + 64),
        ("steps", True),
        ("observation_steps", [0, 1024.5]),
        ("energy_tolerance", 0.01),
        ("initial_temperature_k", 275.15),
        ("latent_heat_j_kg", 1.0),
        ("density_vapor_kg_m3", 1.0),
        ("moisture_risk", {"assessment": "missing", "reason": ""}),
    ):
        changed = {**spec, key: value}
        with pytest.raises((ValueError, TypeError)):
            bridge.validate(changed, fem)
        result = subprocess.run(
            [binary, "case", "validate-freezing-reference", "/dev/stdin"],
            input=json.dumps(changed).encode(),
            capture_output=True,
            check=False,
        )
        assert result.returncode


def test_checked_receipt_cannot_replace_native_metadata_or_byte_identity(tmp_path):
    spec, native, bridge, fem = manufactured(tmp_path)
    native["numerical_verification"] = (
        "independent complete-field Stefan/mass/energy gate required"
    )
    path = tmp_path / "native-freezing-receipt.json"
    path.write_text(json.dumps(native))
    checks, observations, hashes = bridge.verify(spec, native, tmp_path, fem)
    hashes.update(
        bridge.scientific_exports(spec, observations, tmp_path, fem, create=True)
    )
    hashes[path.name] = hashlib.sha256(path.read_bytes()).hexdigest()
    checked = {
        **native,
        "numerical_verification": checks,
        "independent_observations": observations,
        "original_files_sha256": hashes,
    }
    assert bridge.verify(spec, checked, tmp_path, fem)[0] == checks
    changed = {**native, "precision": "float32"}
    path.write_text(json.dumps(changed))
    # Merely replacing the native hash cannot authorize different original metadata.
    hashes[path.name] = hashlib.sha256(path.read_bytes()).hexdigest()
    with pytest.raises(ValueError, match="exact native metadata"):
        bridge.verify(spec, checked, tmp_path, fem)


def test_portable_vtk_keeps_every_native_float64_point_units_coordinates_and_physical_time(
    tmp_path,
):
    spec, receipt, bridge, fem = manufactured(tmp_path)
    _, observations, _ = bridge.verify(spec, receipt, tmp_path, fem)
    hashes = bridge.scientific_exports(spec, observations, tmp_path, fem, create=True)
    root = ET.fromstring((tmp_path / "freezing-1024.vti").read_bytes())
    image = root.find("ImageData")
    assert image.attrib["WholeExtent"] == "0 31 0 3 0 0"
    dx = spec["size_m"][0] / 32
    assert list(map(float, image.attrib["Origin"].split())) == [0, dx / 2, 0]
    assert list(map(float, image.attrib["Spacing"].split())) == [
        dx,
        dx,
        spec["size_m"][2],
    ]
    points = image.find("Piece/PointData")
    originals = list(csv.DictReader((tmp_path / "freezing-1024.csv").open()))
    for field in points.findall("DataArray"):
        name = field.attrib["Name"]
        assert list(map(float, field.text.split())) == [
            float(row[name]) for row in originals
        ]
        if name in {"temperature_k", "specific_enthalpy_j_kg", "liquid_fraction"}:
            assert field.attrib["type"] == "Float64"
            assert (
                field.attrib["unit"]
                == {
                    "temperature_k": "K",
                    "specific_enthalpy_j_kg": "J/kg",
                    "liquid_fraction": "1",
                }[name]
            )
    times = ET.fromstring((tmp_path / "freezing.pvd").read_bytes()).findall(
        "Collection/DataSet"
    )
    assert [(float(v.attrib["timestep"]), v.attrib["file"]) for v in times] == [
        (0, "freezing-0.vti"),
        (observations[-1]["physical_time_s"], "freezing-1024.vti"),
    ]
    assert bridge.scientific_exports(spec, observations, tmp_path, fem) == hashes
    path = tmp_path / "freezing-1024.vti"
    path.write_bytes(path.read_bytes().replace(b'unit="K"', b'unit="degC"'))
    with pytest.raises(ValueError, match="differ from original native"):
        bridge.scientific_exports(spec, observations, tmp_path, fem)
