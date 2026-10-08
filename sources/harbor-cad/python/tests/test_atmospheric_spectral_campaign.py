"""Independent complete native packet reconstruction and mutation rejection."""

import copy
import csv
import importlib.util
import math
from pathlib import Path

import pytest
from test_atmospheric_spectral import adapter, fixture


def campaign_module(monkeypatch):
    scripts = Path(__file__).parents[2] / "scripts"
    monkeypatch.syspath_prepend(str(scripts))
    loader = importlib.util.spec_from_file_location(
        "atmospheric_spectral_campaign", scripts / "verify_atmospheric_spectral_cpu.py"
    )
    campaign = importlib.util.module_from_spec(loader)
    loader.loader.exec_module(campaign)
    return campaign


@pytest.fixture
def packet_campaign(monkeypatch, tmp_path):
    native = adapter(monkeypatch)
    campaign = campaign_module(monkeypatch)
    atmosphere, receiver, raw = fixture()
    receiver["samples"] = 1024
    normalized = native.normalize_source(atmosphere, receiver, raw)
    header = [
        "sample",
        "knot_offset",
        "x_m",
        "y_m",
        "z_m",
        "towards_source_x",
        "towards_source_y",
        "towards_source_z",
        "native_cosine",
        "native_emitter_id",
        "native_pdf",
        *[f"native_weight_w_m2_nm_{i}" for i in range(4)],
    ]
    active = [e for e in normalized["diffuse_emitters"] if any(e["irradiance_w_m2_nm"])]
    rows = []
    for sample in range(receiver["samples"]):
        emitter = active[sample % len(active)]
        rows.append(
            [
                sample,
                0,
                0,
                0,
                0,
                *[-v for v in emitter["propagation_direction"]],
                native.cosine(
                    receiver["sensor_normal"], emitter["propagation_direction"]
                ),
                emitter["id"],
                1 / len(active),
                *[emitter["irradiance_w_m2_nm"][0] * len(active)] * 4,
            ]
        )

    def write(path, values):
        with path.open("w", newline="") as handle:
            writer = csv.writer(handle)
            writer.writerow(header)
            writer.writerows(values)

    diffuse = tmp_path / "diffuse-1.csv"
    direct = tmp_path / "direct-1.csv"
    write(diffuse, rows)
    write(
        direct, [[sample, 0, *[0] * 7, "none", 0, *[0] * 4] for sample in range(1024)]
    )
    components = {}
    originals = []
    for name, path in (("direct", direct), ("diffuse", diffuse)):
        values = campaign.reconstruct(path, receiver, normalized, name, native)
        components[name] = {
            "native_channels_w_m2": values,
            "numerical_verification": native.spectral_bridge.verify_channels(
                {**normalized, "reference": normalized["component_references"][name]},
                values,
                receiver["relative_tolerance"],
            ),
        }
        originals.append(
            {
                "component": name,
                "path": path.name,
                "sha256": campaign.checksum(path),
                "bytes": path.stat().st_size,
            }
        )
    combined = {
        name: math.fsum(v["native_channels_w_m2"][name] for v in components.values())
        for name in normalized["weights"]
    }
    observation = {
        "seed": 1,
        "samples": receiver["samples"],
        "originals": originals,
        "components": components,
        "native_channels_w_m2": combined,
        "numerical_verification": native.spectral_bridge.verify_channels(
            normalized, combined, receiver["relative_tolerance"]
        ),
        "exposure_j_m2": {
            name: value * normalized["history_integral_s"]
            for name, value in combined.items()
        },
        "native_power_w": {
            name: value * normalized["sensor_area_m2"]
            for name, value in combined.items()
        },
        "energy_j": {
            name: value
            * normalized["history_integral_s"]
            * normalized["sensor_area_m2"]
            for name, value in combined.items()
        },
    }
    return campaign, native, receiver, normalized, observation, header, rows, write


@pytest.mark.parametrize(
    "mutation", ["empty", "missing", "duplicate", "filename", "components"]
)
def test_observation_requires_both_canonical_original_components(
    packet_campaign, tmp_path, mutation
):
    campaign, native, receiver, normalized, observation, _, _, _ = packet_campaign
    assert (
        campaign.verify_observation(tmp_path, observation, receiver, normalized, native)
        == observation["native_channels_w_m2"]
    )
    changed = copy.deepcopy(observation)
    if mutation == "empty":
        changed["originals"] = []
    elif mutation == "missing":
        changed["originals"] = changed["originals"][:1]
    elif mutation == "duplicate":
        changed["originals"] = changed["originals"] * 2
    elif mutation == "filename":
        (tmp_path / "diffuse-1.csv").rename(tmp_path / "renamed.csv")
        changed["originals"][1]["path"] = "renamed.csv"
    else:
        changed["components"]["extra"] = {
            "native_channels_w_m2": {name: 0.0 for name in normalized["weights"]}
        }
    with pytest.raises(ValueError):
        campaign.verify_observation(tmp_path, changed, receiver, normalized, native)


def test_packet_rejects_compensated_probability_and_reciprocal_weight_changes(
    packet_campaign, tmp_path
):
    campaign, native, receiver, normalized, _, header, rows, write = packet_campaign
    changed = copy.deepcopy(rows)
    source = normalized["diffuse_emitters"][0]["irradiance_w_m2_nm"][0]
    # These two samples have the same emitter/cosine and preserve every band sum.
    for sample, pdf in ((0, 0.5), (64, 1 / 126)):
        changed[sample][header.index("native_pdf")] = pdf
        for lane in range(4):
            changed[sample][header.index(f"native_weight_w_m2_nm_{lane}")] = (
                source / pdf
            )
    path = tmp_path / "diffuse-1.csv"
    write(path, changed)
    with pytest.raises(ValueError):
        campaign.reconstruct(path, receiver, normalized, "diffuse", native)


@pytest.mark.parametrize("axis,position", [("x_m", 0.00075), ("y_m", 0.00105)])
def test_packet_rejects_points_inside_circle_but_outside_sensor_rectangle(
    packet_campaign, tmp_path, axis, position
):
    campaign, native, receiver, normalized, _, header, rows, write = packet_campaign
    changed = copy.deepcopy(rows)
    changed[3][header.index(axis)] = position
    path = tmp_path / "diffuse-1.csv"
    write(path, changed)
    with pytest.raises(ValueError):
        campaign.reconstruct(path, receiver, normalized, "diffuse", native)


def test_native_campaign_covers_positive_direct_empty_diffuse_and_varying_multiknot_sources(
    monkeypatch,
):
    campaign = campaign_module(monkeypatch)
    native = adapter(monkeypatch)
    cases = campaign.reference_cases(Path(__file__).parents[2])
    references = {
        name: native.normalize_source(atmosphere, receiver, original)
        for name, atmosphere, receiver, original in cases
    }
    for name in ("direct-up", "direct-inclined", "mixed-up", "multiknot-inclined"):
        assert references[name]["component_references"]["direct"]["incident"] > 0
    assert not any(
        any(e["irradiance_w_m2_nm"])
        for e in references["direct-up"]["diffuse_emitters"]
    )
    multi = references["multiknot-inclined"]
    assert len(multi["wavelengths_nm"]) == 6
    assert len(set(multi["weights"]["absorbed"])) > 1
    assert len(set(multi["weights"]["ageing"])) > 1
    assert multi["direct_emitter"]["irradiance_w_m2_nm"][1] == 0.0
    assert len({sum(e["irradiance_w_m2_nm"]) for e in multi["diffuse_emitters"]}) > 1


def test_unequal_source_probabilities_preserve_every_multiknot_packet(
    monkeypatch, tmp_path
):
    campaign = campaign_module(monkeypatch)
    native = adapter(monkeypatch)
    _, atmosphere, receiver, original = next(
        case
        for case in campaign.reference_cases(Path(__file__).parents[2])
        if case[0] == "multiknot-inclined"
    )
    receiver["samples"] = 1024
    normalized = native.normalize_source(atmosphere, receiver, original)
    active = [e for e in normalized["diffuse_emitters"] if any(e["irradiance_w_m2_nm"])]
    emitter = active[0]
    pmf = sum(emitter["irradiance_w_m2_nm"]) / sum(
        sum(e["irradiance_w_m2_nm"]) for e in active
    )
    cosine = native.cosine(receiver["sensor_normal"], emitter["propagation_direction"])
    header = [
        "sample",
        "knot_offset",
        "x_m",
        "y_m",
        "z_m",
        "towards_source_x",
        "towards_source_y",
        "towards_source_z",
        "native_cosine",
        "native_emitter_id",
        "native_pdf",
        *[f"native_weight_w_m2_nm_{i}" for i in range(4)],
    ]
    rows = [
        [
            sample,
            offset,
            0,
            0,
            0,
            *[-v for v in emitter["propagation_direction"]],
            cosine,
            emitter["id"],
            pmf,
            *[
                emitter["irradiance_w_m2_nm"][min(offset + lane, 5)] / pmf
                for lane in range(4)
            ],
        ]
        for sample in range(1024)
        for offset in (0, 4)
    ]
    path = tmp_path / "multiknot.csv"
    with path.open("w", newline="") as handle:
        writer = csv.writer(handle)
        writer.writerow(header)
        writer.writerows(rows)
    actual = campaign.reconstruct(path, receiver, normalized, "diffuse", native)
    expected = {
        name: native.spectral_bridge.product_integral(
            normalized["wavelengths_nm"],
            [v * cosine / pmf for v in emitter["irradiance_w_m2_nm"]],
            weights,
        )
        for name, weights in normalized["weights"].items()
    }
    assert all(
        math.isclose(actual[name], value, rel_tol=1e-14)
        for name, value in expected.items()
    )


def test_complete_packet_reconstruction_rejects_changed_phi_cosine_pmf_and_coverage(
    monkeypatch, tmp_path
):
    native = adapter(monkeypatch)
    atmosphere, receiver, _ = fixture()
    for spec in (atmosphere, receiver):
        spec["wavelengths"] = [{"value": 280 + 24 * k, "unit": "nm"} for k in range(6)]
    atmosphere["toa_irradiance"] = [{"value": 1000.0, "unit": "W/(m2*nm)"}] * 6
    receiver["source"]["irradiance"] = atmosphere["toa_irradiance"]
    receiver.update(absorptivity=[0.5] * 6, ageing_action=[0.25] * 6, samples=1024)
    campaign = campaign_module(monkeypatch)
    normalized = native.normalize_source(
        atmosphere, receiver, campaign.original_fields(atmosphere)
    )
    # Manufactured packet scaffolding tests reconstruction only, not sampling or
    # native execution. The full campaign retains independent scientific gates.
    emitter = normalized["diffuse_emitters"][0]
    weight = emitter["irradiance_w_m2_nm"][0] * 64
    cosine = 0.9375
    header = [
        "sample",
        "knot_offset",
        "x_m",
        "y_m",
        "z_m",
        "towards_source_x",
        "towards_source_y",
        "towards_source_z",
        "native_cosine",
        "native_emitter_id",
        "native_pdf",
        *[f"native_weight_w_m2_nm_{i}" for i in range(4)],
    ]
    rows = [
        [
            sample,
            offset,
            0,
            0,
            0,
            *[-v for v in emitter["propagation_direction"]],
            cosine,
            emitter["id"],
            1 / 64,
            *[weight] * 4,
        ]
        for sample in range(1024)
        for offset in (0, 4)
    ]
    path = tmp_path / "packets.csv"

    def write(values):
        with path.open("w", newline="") as handle:
            writer = csv.writer(handle)
            writer.writerow(header)
            writer.writerows(values)

    write(rows)
    reconstructed = campaign.reconstruct(path, receiver, normalized, "diffuse", native)
    expected = 120 * weight * cosine
    assert math.isclose(reconstructed["incident"], expected, rel_tol=1e-14)
    assert reconstructed["absorbed"] == reconstructed["incident"] / 2
    for column, value in (
        ("native_emitter_id", "angular-000-001"),
        ("native_cosine", 0.1),
        ("native_pdf", 0.5),
        ("native_weight_w_m2_nm_0", float("nan")),
    ):
        changed = copy.deepcopy(rows)
        changed[3][header.index(column)] = value
        write(changed)
        with pytest.raises(ValueError):
            campaign.reconstruct(path, receiver, normalized, "diffuse", native)
    for values in (rows[:-1], rows + [rows[-1]]):
        write(values)
        with pytest.raises(ValueError):
            campaign.reconstruct(path, receiver, normalized, "diffuse", native)
