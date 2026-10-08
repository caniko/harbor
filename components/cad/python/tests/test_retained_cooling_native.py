"""Manufactured original-control checks cannot promote a reconstructed solve."""

import copy
import csv
import hashlib
import importlib.util
import io
import json
import math
from pathlib import Path

import pytest


def adapter():
    path = Path(__file__).parents[2] / "adapters/retained_cooling_reference.py"
    loader = importlib.util.spec_from_file_location("retained_cooling_reference", path)
    module = importlib.util.module_from_spec(loader)
    loader.loader.exec_module(module)
    return module


def csv_bytes(columns, records):
    stream = io.StringIO()
    writer = csv.writer(stream, lineterminator="\n")
    writer.writerow(columns)
    writer.writerows(records)
    return stream.getvalue().encode()


def manufactured(root, q=1):
    native = adapter()
    dx, density, cp, latent, cold, melting = (
        2e-6,
        1000.0,
        4180.0,
        334400.0,
        265.15,
        273.15,
    )
    original = csv_bytes(
        ["x_m", "y_m", "material", "phi", "u_lattice", "v_lattice"],
        [
            [
                i * dx,
                j * dx,
                1 if j in (1, 2) else 2,
                (1.0, 0.5, 0.0)[i] if j in (1, 2) else 1.0,
                0,
                0,
            ]
            for j in range(4)
            for i in range(3)
        ],
    )
    thermal = {
        "density_kg_m3": density,
        "specific_heat_j_kg_k": cp,
        "conductivity_w_m_k": 0.6,
        "latent_heat_j_kg": latent,
        "melting_temperature_k": melting,
        "initial_temperature_k": melting,
        "cold_wall_temperature_k": cold,
        "material_temperature_domain_k": [250.0, 300.0],
        "stefan_number_at_full_water_fraction": 0.1,
    }
    spec = {
        "schema_version": 1,
        "synthetic": True,
        "formulation": "stationary_equal_property_retained_phase_conduction",
        "source_shape": [3, 4],
        "spacing_m": dx,
        "extrusion_m": 0.001,
        "destination_origin_m": [0.1, -0.2, 0.3],
        "thermal": thermal,
        "steps": 1,
        "observation_steps": [0, 1],
        "integration_substeps": 1,
        "spatial_refinement": q,
    }
    normalized = native.normalize(spec, original)
    mass = normalized["cell_mass_kg"]
    dt = normalized["physical_step_s"]
    summaries = []
    for step in (0, 1):
        rows = []
        for j in range(1, 2 * q + 1):
            for i in range(3 * q):
                parent = i // q
                f = parent / 2
                phase = (1.0 if step == 0 else 0.9) if f else 0.0
                t = melting if f or step == 0 else melting - 0.01
                h = cp * (t - cold) + f * latent * phase
                rows.append(
                    [
                        i,
                        j,
                        parent,
                        (j - 1) // q + 1,
                        0.1 + parent * dx + ((i % q + 0.5) / q - 0.5) * dx,
                        -0.2
                        + ((j - 1) // q + 1) * dx
                        + (((j - 1) % q + 0.5) / q - 0.5) * dx,
                        f,
                        h,
                        t,
                        phase,
                    ]
                )
        path = f"cooling-{step}.csv"
        (root / path).write_bytes(csv_bytes(native.COLUMNS, rows))
        energy = mass * math.fsum(row[7] for row in rows)
        summaries.append(
            {
                "step": step,
                "path": path,
                "time_s": step * dt,
                "energy_j": energy,
                "water_mass_kg": mass * math.fsum(row[6] for row in rows),
                "liquid_water_mass_kg": mass
                * math.fsum(row[6] * row[9] for row in rows),
                "cold_exchange_j": energy - normalized["initial_energy_j"],
                "reflecting_exchange_j": 0.0,
            }
        )
    (root / "heat-exchange.csv").write_bytes(
        csv_bytes(
            ["step", "time_s", "cold_exchange_j", "reflecting_exchange_j"],
            [[1, dt, summaries[1]["cold_exchange_j"], 0.0]],
        )
    )
    receipt = {
        "schema_version": 1,
        "adapter": "OpenLB",
        "backend": "cpu",
        "precision": "float64",
        "source_revision": "145cd54810b468f4b6fd3ed86b10644264841578",
        "collision": "native_total_enthalpy_trt",
        "trt_magic": 0.25,
        "executed": True,
        "software_fallback": False,
        "request": spec,
        "source_spacing_m": dx,
        "spacing_m": dx / q,
        "physical_step_s": dt,
        "cell_mass_kg": mass,
        "source_shape": [3, 4],
        "native_shape": [3 * q, 2 * q + 2],
        "boundary": "native_half_link_cold_ymin; native_half_link_insulated_ymax; periodic_x",
        "physical_validation": "unqualified",
        "snapshots": summaries,
    }
    return native, spec, original, receipt, normalized


@pytest.mark.parametrize("q", (1, 2, 3, 4))
def test_complete_parent_subcontrols_keep_source_water_and_sensible_background(
    tmp_path, q
):
    native, spec, original, receipt, normalized = manufactured(tmp_path, q)
    before = {p.name: p.read_bytes() for p in tmp_path.iterdir()}
    observed = native.verify(spec, original, receipt, tmp_path)
    assert observed["executed"] is False
    assert all(check["passed"] for check in observed["checks"].values())
    assert observed["observations"][0]["water_mass_kg"] == pytest.approx(
        normalized["water_mass_kg"], rel=1e-14
    )
    assert observed["observations"][1]["solid_water_mass_kg"] == pytest.approx(
        0.1 * normalized["water_mass_kg"], rel=1e-14
    )
    assert len(observed["original_files_sha256"]) == 3
    assert {p.name: p.read_bytes() for p in tmp_path.iterdir()} == before


def test_original_stationarity_overshoots_missing_controls_and_injected_thermal_state_refuse(
    tmp_path,
):
    native, spec, original, _, _ = manufactured(tmp_path)
    for changed in (
        original.replace(b"0,0\n", b"0.1,0\n", 1),
        original.replace(b"0.5,0,0", b"1.0001,0,0"),
        b"\n".join(original.splitlines()[:-1]) + b"\n",
    ):
        with pytest.raises(ValueError):
            native.normalize(spec, changed)
    for key, value in (
        ("initial_temperature_k", 274.0),
        ("material_temperature_domain_k", [270.0, 300.0]),
        ("stefan_number_at_full_water_fraction", 0.2),
    ):
        changed = copy.deepcopy(spec)
        changed["thermal"][key] = value
        with pytest.raises(ValueError):
            native.normalize(changed, original)
    for changed in (
        {**spec, "execute": True},
        {**spec, "steps": True},
        {**spec, "observation_steps": [0, 0, 1]},
    ):
        with pytest.raises(ValueError):
            native.normalize(changed, original)


@pytest.mark.parametrize(
    "mutation",
    (
        "parent",
        "water",
        "position",
        "enthalpy",
        "time",
        "boundary",
        "extra",
        "summary",
        "weak_gate",
        "symlink",
    ),
)
def test_mutated_complete_originals_or_boundary_exchange_never_verify(
    tmp_path, mutation
):
    native, spec, original, receipt, _ = manufactured(tmp_path)
    original_before = original
    field = tmp_path / "cooling-1.csv"
    if mutation in {"parent", "water", "position", "enthalpy"}:
        rows = list(csv.reader(io.StringIO(field.read_text())))
        index = {"parent": 2, "water": 6, "position": 4, "enthalpy": 7}[mutation]
        rows[1][index] = str(float(rows[1][index]) + 1.0)
        field.write_bytes(csv_bytes(rows[0], rows[1:]))
    elif mutation == "time":
        receipt["snapshots"][1]["time_s"] *= 2
    elif mutation == "boundary":
        ledger = tmp_path / "heat-exchange.csv"
        rows = list(csv.reader(io.StringIO(ledger.read_text())))
        rows[1][2] = "0"
        ledger.write_bytes(csv_bytes(rows[0], rows[1:]))
    elif mutation == "extra":
        field.write_bytes(
            field.read_bytes() + field.read_bytes().splitlines()[1] + b"\n"
        )
    elif mutation == "summary":
        receipt["snapshots"][1]["energy_j"] *= 1.1
    elif mutation == "symlink":
        data = field.read_bytes()
        target = tmp_path / "other.csv"
        target.write_bytes(data)
        field.unlink()
        field.symlink_to(target)
    before = {p.name: p.read_bytes() for p in tmp_path.iterdir() if p.is_file()}
    with pytest.raises(ValueError):
        native.verify(
            spec,
            original,
            receipt,
            tmp_path,
            conservation_tolerance=0.02 if mutation == "weak_gate" else 1e-10,
        )
    assert (
        original == original_before
        and {p.name: p.read_bytes() for p in tmp_path.iterdir() if p.is_file()}
        == before
    )


def bridge(monkeypatch):
    directory = Path(__file__).parents[2] / "adapters"
    monkeypatch.syspath_prepend(str(directory))
    loader = importlib.util.spec_from_file_location(
        "retained_cooling_bridge", directory / "retained_cooling_bridge.py"
    )
    module = importlib.util.module_from_spec(loader)
    loader.loader.exec_module(module)
    return module


def test_bridge_requires_exact_source_bytes_and_nonweakened_approval(
    monkeypatch, tmp_path
):
    native, spec, original, _, _ = manufactured(tmp_path)
    packaged = bridge(monkeypatch)
    envelope = {
        "schema_version": 1,
        "native_request": spec,
        "original_sha256": hashlib.sha256(original).hexdigest(),
        "original_bytes": len(original),
        "maximum_relative_conservation_error": 1e-10,
    }
    assert packaged.validate_envelope(envelope, original) == native.normalize(
        spec, original
    )
    for key, value in (
        ("schema_version", True),
        ("original_bytes", len(original) + 1),
        ("original_sha256", "0" * 64),
        ("maximum_relative_conservation_error", 0.02),
        ("source_temperature", None),
    ):
        changed = {**envelope, key: value}
        with pytest.raises(ValueError):
            packaged.validate_envelope(changed, original)
    with pytest.raises(ValueError):
        packaged.validate_envelope(envelope, original + b"changed")


def test_bridge_preserves_scientific_leftovers_and_refuses_writable_source_before_dispatch(
    monkeypatch, tmp_path
):
    _, spec, original, _, _ = manufactured(tmp_path)
    packaged = bridge(monkeypatch)
    envelope = {
        "schema_version": 1,
        "native_request": spec,
        "original_sha256": hashlib.sha256(original).hexdigest(),
        "original_bytes": len(original),
        "maximum_relative_conservation_error": 1e-10,
    }
    before = {p.name: p.read_bytes() for p in tmp_path.iterdir()}
    with pytest.raises(ValueError, match="fresh native cooling"):
        packaged.fresh_output(tmp_path)
    monkeypatch.setattr(
        packaged.sys,
        "argv",
        ["harbor-cad-retained-cooling", "reference", "/inputs/request.json"],
    )
    monkeypatch.setattr(
        packaged,
        "read_regular",
        lambda path, limit: (
            json.dumps(envelope).encode()
            if str(path) == "/inputs/request.json"
            else original
        ),
    )
    monkeypatch.setattr(packaged.common, "cpu_sandbox", lambda *args: {"checks": {}})
    from types import SimpleNamespace

    monkeypatch.setattr(packaged.os, "statvfs", lambda path: SimpleNamespace(f_flag=0))
    with pytest.raises(ValueError, match="read-only authoritative source"):
        packaged.main()
    assert {p.name: p.read_bytes() for p in tmp_path.iterdir()} == before


def test_published_verification_cannot_substitute_native_summary_or_source(tmp_path):
    native, spec, original, receipt, _ = manufactured(tmp_path)
    raw = json.dumps(receipt).encode()
    (tmp_path / "native-retained-cooling-receipt.json").write_bytes(raw)
    published = copy.deepcopy(receipt)
    published["independent_verification"] = native.verify(
        spec, original, receipt, tmp_path
    )
    published["original_source_sha256"] = hashlib.sha256(original).hexdigest()
    published["native_receipt_sha256"] = hashlib.sha256(raw).hexdigest()
    native.verify(spec, original, published, tmp_path)
    for field in ("original_source_sha256", "native_receipt_sha256"):
        changed = {**published, field: "0" * 64}
        with pytest.raises(ValueError):
            native.verify(spec, original, changed, tmp_path)
    changed = copy.deepcopy(published)
    changed["independent_verification"]["checks"]["energy_balance_relative_error"][
        "error"
    ] = 0.02
    with pytest.raises(ValueError):
        native.verify(spec, original, changed, tmp_path)


def test_initial_identity_and_evolved_refinement_have_distinct_strict_gates():
    path = Path(__file__).parents[2] / "scripts/verify_retained_cooling_history.py"
    loader = importlib.util.spec_from_file_location("cooling_history", path)
    module = importlib.util.module_from_spec(loader)
    loader.loader.exec_module(module)
    names = ["coarse", "middle", "fine"]
    fields = {
        "coarse": {0: {(0, 1): (1.0, 273.15, 1.0)}, 1: {(0, 1): (1.01, 270.01, 0.21)}},
        "middle": {
            0: {(0, 1): (1.0000000000000002, 273.15, 1.0)},
            1: {(0, 1): (1.005, 270.005, 0.205)},
        },
        "fine": {0: {(0, 1): (1.0, 273.15, 1.0)}, 1: {(0, 1): (1.0, 270.0, 0.2)}},
    }
    observed = module.refinements(fields, names, 8.0)
    assert observed["passed"]
    assert observed["assessments"][0]["assessment"] == "initial_identity"
    assert observed["assessments"][0]["tolerance"] == 1e-10
    fields["middle"][0][(0, 1)] = (1.000001, 273.15, 1.0)
    assert not module.refinements(fields, names, 8.0)["passed"]
    fields["middle"][0][(0, 1)] = fields["fine"][0][(0, 1)]
    fields["middle"][1][(0, 1)] = (1.015, 270.015, 0.215)
    assert not module.refinements(fields, names, 8.0)["passed"]
