"""Independent native input and spectral-product checks without importing renderer."""

import copy
import csv
import importlib.util
import json
import math
from pathlib import Path
from types import SimpleNamespace

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


def test_native_directory_accepts_worker_capture_log_but_never_reuses_scientific_output(
    tmp_path,
):
    native = adapter()
    native.require_new_work(tmp_path)
    (tmp_path / "spectral.log").write_text("owned worker launch diagnostics\n")
    native.require_new_work(tmp_path)
    (tmp_path / "directional-1.csv").write_text("old native field\n")
    with pytest.raises(ValueError):
        native.require_new_work(tmp_path)
    (tmp_path / "directional-1.csv").unlink()
    (tmp_path / "spectral.log").unlink()
    (tmp_path / "spectral.log").symlink_to(tmp_path / "absent")
    with pytest.raises(ValueError):
        native.require_new_work(tmp_path)


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


def test_directional_reduction_matches_fsum_of_retained_float32_knots_at_maximum_budget(
    tmp_path,
):
    # ABI-free observation scaffolding, not native execution evidence. Its
    # non-dyadic Float32 product exposes accumulated ordinary-sum roundoff.
    native = adapter()
    spec = fixture()
    spec["samples"] = 65536
    normalized = native.normalize(spec)
    cosine = 0.800000011920929
    weights = [0.10000000149011612, 0.30000001192092896]
    position = SimpleNamespace(p=[0.0, 0.0, 0.0], n=[0.0, 0.0, 1.0])
    shape = SimpleNamespace(sensor=lambda: True, sample_position=lambda *_: position)
    scene = SimpleNamespace(
        shapes=lambda: [shape],
        sample_emitter_direction=lambda *_: (
            SimpleNamespace(d=[0, 0, 1]),
            weights + weights,
        ),
    )
    mi = SimpleNamespace(
        SurfaceInteraction3f=SimpleNamespace, Frame3f=lambda n: n, Spectrum=lambda v: v
    )
    dr = SimpleNamespace(dot=lambda *_: cosine)
    path = tmp_path / "scaffold.csv"
    measured = native.measure_directional(scene, spec, normalized, 1, path, mi, dr)
    with path.open() as handle:
        rows = list(csv.DictReader(handle))
    means = [
        math.fsum(
            float(row[f"emitter_weight_w_m2_nm_{i}"]) * float(row["native_cosine"])
            for row in rows
        )
        / len(rows)
        for i in range(2)
    ]
    for channel, optical_weights in normalized["weights"].items():
        expected = native.product_integral(
            normalized["wavelengths_nm"], means, optical_weights
        )
        assert math.isclose(measured[channel], expected, rel_tol=2e-15)


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


def reflection_fixture():
    incident = fixture()
    incident["source"] = {
        "kind": "isotropic",
        "radiance": [
            {"value": 1.0, "unit": "W/(m2*sr*nm)"},
            {"value": 3.0, "unit": "W/(m2*sr*nm)"},
        ],
    }
    incident["sensor_normal"] = [0, 0, -1]
    return {
        "schema_version": 1,
        "formulation": "isotropic_lambertian_disk",
        "incident": incident,
        "disk_radius": {"value": 10.0, "unit": "m"},
        "sensor_height": {"value": 1.0, "unit": "m"},
        "reflectance": 0.4,
        "reflectance_provenance": "synthetic explicit constant UV reflectance; no visible material inference",
        "geometry_provenance": "centred downward black sensor and finite upward circular disk with surrounding isotropic environment",
        "maximum_model_error": 1e-5,
    }


def test_independent_reflection_retains_uncovered_environment_and_separate_model_error_bounds():
    native = adapter()
    raw = reflection_fixture()
    normal = native.normalize(raw)
    factor = 1.0 - 0.6 * 100 / 101
    assert math.isclose(
        normal["reference"]["absorbed"], 132 * math.pi * factor, rel_tol=1e-14
    )
    assert normal["reflection"]["model_relative_error_bound"] < 1e-5
    assert normal["reflection"]["black_sensor_shadow_relative_error_bound"] > 0.0
    raw["reflectance"] = 0.0
    raw["incident"]["sensor_width"] = {"value": 1e-5, "unit": "m"}
    raw["incident"]["sensor_height"] = {"value": 1e-5, "unit": "m"}
    assert native.normalize(raw)["reference"]["incident"] > 0.0
    raw["reflectance"] = 1.0
    assert math.isclose(
        native.normalize(raw)["reference"]["incident"], 240 * math.pi, rel_tol=1e-14
    )
    for field, value in (
        ("maximum_model_error", 0.02),
        ("reflectance", 1.1),
        ("reflectance_provenance", ""),
        ("schema_version", 2),
        ("reflection", None),
    ):
        rejected = reflection_fixture()
        rejected[field] = value
        with pytest.raises(ValueError):
            native.normalize(rejected)
    rejected = reflection_fixture()
    rejected["incident"]["source"] = fixture()["source"]
    with pytest.raises(ValueError):
        native.normalize(rejected)
    rejected = reflection_fixture()
    rejected["incident"]["sensor_width"]["value"] = 10.0
    rejected["incident"]["sensor_height"]["value"] = 10.0
    with pytest.raises(ValueError):
        native.normalize(rejected)


@pytest.mark.parametrize(
    "mutation", [None, "swapped_values", "duplicate", "format", "size"]
)
def test_exr_roundtrip_matches_named_float32_channels_without_positional_reinterpretation(
    mutation,
):
    native = adapter()
    original = {
        "incident": 753.3287353515625,
        "absorbed": 413.88055419921875,
        "ageing": 533.985107421875,
    }
    names = ["absorbed", "ageing", "incident"]
    values = [original[name] for name in names]
    if mutation == "swapped_values":
        values[0], values[1] = values[1], values[0]
    if mutation == "duplicate":
        names[1] = names[0]
    bitmap = SimpleNamespace(
        struct_=lambda: [SimpleNamespace(name=name) for name in names],
        size=lambda: [2, 1] if mutation == "size" else [1, 1],
        component_format=lambda: "float16" if mutation == "format" else "float32",
    )
    mi = SimpleNamespace(
        Struct=SimpleNamespace(Type=SimpleNamespace(Float32="float32")),
        TensorXf=lambda _: SimpleNamespace(array=values),
    )
    if mutation:
        with pytest.raises(ValueError):
            native.verify_exr_roundtrip(original, bitmap, mi)
    else:
        assert native.verify_exr_roundtrip(original, bitmap, mi) == original
