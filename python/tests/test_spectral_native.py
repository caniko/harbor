"""Independent native input and spectral-product checks without importing renderer."""

import copy
import importlib.util
import json
import math
from pathlib import Path

import pytest


def adapter():
    spec = importlib.util.spec_from_file_location(
        "spectral_native_checks",
        Path(__file__).parents[2] / "adapters/spectral_reference.py",
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def fixture():
    return json.loads(
        (Path(__file__).parents[2] / "examples/spectral-reference.json").read_text()
    )


def test_independent_uv_quadrature_preserves_absorbed_power_dose_and_original_si_units():
    native = adapter()
    normalized = native.normalize(fixture())
    assert math.isclose(normalized["reference"]["incident"], 240.0, rel_tol=1e-14)
    assert math.isclose(normalized["reference"]["absorbed"], 132.0, rel_tol=1e-14)
    assert math.isclose(normalized["reference"]["ageing"], 170.0, rel_tol=1e-14)
    assert normalized["history_integral_s"] == 3600.0
    assert normalized["source_unit"] == "W/(m2*nm)"
    assert normalized["wavelengths_nm"] == [280.0, 400.0]
    assert normalized["source_values_nm"] == [1.0, 3.0]
    raw = fixture()
    raw["wavelengths"] = [
        {"value": 280e-9, "unit": "m"},
        {"value": 400e-9, "unit": "m"},
    ]
    raw["source"]["irradiance"] = [
        {"value": 1e9, "unit": "W/(m2*m)"},
        {"value": 3e9, "unit": "W/(m2*m)"},
    ]
    assert math.isclose(
        native.normalize(raw)["reference"]["absorbed"], 132.0, rel_tol=1e-14
    )
    raw = fixture()
    raw["sensor_normal"] = [0.6, 0.0, 0.8]
    assert math.isclose(
        native.normalize(raw)["reference"]["incident"], 192.0, rel_tol=1e-14
    )
    raw["occlusion"] = "full_directional_occluder"
    assert native.normalize(raw)["reference"]["incident"] == 0.0
    raw = fixture()
    raw["source"] = {
        "kind": "isotropic",
        "radiance": [
            {"value": 1.0, "unit": "W/(m2*sr*nm)"},
            {"value": 3.0, "unit": "W/(m2*sr*nm)"},
        ],
    }
    assert math.isclose(
        native.normalize(raw)["reference"]["absorbed"], 132 * math.pi, rel_tol=1e-14
    )


@pytest.mark.parametrize(
    "field,value",
    [
        ("precision", "Float64"),
        ("variant", "cuda_ad_spectral"),
        ("relative_tolerance", 0.1),
        ("samples", 4095),
        ("absorptivity", [0.2, 1.1]),
        ("seeds", [1, 1, 3]),
        ("optical_provenance", ""),
        ("sensor_normal", [0, 0, 2]),
        ("temperature_k", 273.15),
    ],
)
def test_native_input_rejects_unapproved_physics_precision_and_numeric_acceptance(
    field, value
):
    raw = fixture()
    raw[field] = value
    with pytest.raises((ValueError, TypeError)):
        adapter().normalize(raw)


def test_native_input_and_field_verification_reject_missing_angular_data_nonfinite_or_changed_gates():
    native = adapter()
    raw = fixture()
    raw["source"]["propagation_direction"] = [0, 0, 0]
    with pytest.raises(ValueError):
        native.normalize(raw)
    raw = fixture()
    raw["source"]["irradiance"][1]["value"] = float("nan")
    with pytest.raises(ValueError):
        native.normalize(raw)
    raw = fixture()
    raw["wavelengths"].reverse()
    with pytest.raises(ValueError):
        native.normalize(raw)
    raw = fixture()
    raw["history"][0]["time"]["value"] = 1
    with pytest.raises(ValueError):
        native.normalize(raw)
    normalized = native.normalize(fixture())
    values = {"incident": 240.0, "absorbed": 132.0, "ageing": 170.0}
    checks = native.verify_channels(normalized, values, 0.02)
    assert checks["passed"] and checks["maximum_normalized_error"] < 1e-14
    bad = copy.deepcopy(values)
    bad["absorbed"] = 156.0
    with pytest.raises(ValueError):
        native.verify_channels(normalized, bad, 0.02)
    bad = copy.deepcopy(values)
    bad["incident"] = float("inf")
    with pytest.raises(ValueError):
        native.verify_channels(normalized, bad, 0.02)
    with pytest.raises(ValueError):
        native.verify_channels(normalized, values, 0.1)
