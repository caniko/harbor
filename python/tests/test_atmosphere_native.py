"""ABI-free independent atmospheric preparation and complete angular-output gates."""

import copy
import importlib.util
import json
import math
from pathlib import Path

import pytest


def adapter():
    path = Path(__file__).parents[2] / "adapters/atmosphere_reference.py"
    spec = importlib.util.spec_from_file_location("atmosphere_checks", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def fixture():
    return json.loads(
        (Path(__file__).parents[2] / "examples/atmosphere-reference.json").read_text()
    )


def test_atmosphere_preserves_explicit_unit_angular_transport_and_transparent_reference():
    module = adapter()
    spec = fixture()
    prepared = module.normalize(spec)
    assert prepared["propagation_direction"] == pytest.approx(
        [0, 0.5, -math.cos(math.pi / 6)]
    )
    assert len(prepared["umu"]) == 64 and len(prepared["phi_deg"]) == 32
    assert 64 * 32 * prepared["angular_cell_solid_angle_sr"] == pytest.approx(
        4 * math.pi
    )
    assert prepared["transparent_horizontal_reference_w_m2_nm"] is None
    spec["model"] = "transparent_reference"
    assert module.normalize(spec)[
        "transparent_horizontal_reference_w_m2_nm"
    ] == pytest.approx([v * math.cos(math.pi / 6) for v in (1, 2, 3)])
    changed = copy.deepcopy(spec)
    for q in changed["wavelengths"]:
        q.update(value=q["value"] * 1e-9, unit="m")
    for q in changed["toa_irradiance"]:
        q.update(value=q["value"] * 1e9, unit="W/(m2*m)")
    assert module.normalize(changed)["toa_irradiance_w_m2_nm"] == pytest.approx(
        [1, 2, 3]
    )


def test_complete_angular_fields_reconstruct_native_flux_without_an_isotropic_source_substitution():
    module = adapter()
    spec = fixture()
    prepared = module.normalize(spec)
    # Independent manufactured radiance varies with propagation cosine. The
    # midpoint sum integrates its polynomial exactly to its discretized reference.
    diffuse = math.fsum(
        (-mu) * (1 + mu * mu) * prepared["angular_cell_solid_angle_sr"]
        for mu in prepared["umu"]
        if mu < 0
        for _ in prepared["phi_deg"]
    )
    rows = [
        " ".join(
            map(
                str,
                [
                    wl,
                    toa * math.cos(math.pi / 6),
                    diffuse,
                    0,
                    *[
                        1 + mu * mu if mu < 0 else 0
                        for mu in prepared["umu"]
                        for _ in prepared["phi_deg"]
                    ],
                ],
            )
        )
        for wl, toa in zip(
            prepared["wavelengths_nm"], prepared["toa_irradiance_w_m2_nm"]
        )
    ]
    raw = "\n".join(rows) + "\n"
    parsed = module.parse_original(raw, spec, prepared)
    assert parsed["maximum_angular_flux_error"] < 1e-12
    assert (
        len(parsed["radiance_w_m2_sr_nm"]) == 3
        and len(parsed["radiance_w_m2_sr_nm"][0]) == 2048
    )
    for bad in (
        raw.rsplit(" ", 1)[0],
        raw.replace("280", "281", 1),
        raw.replace(str(diffuse), str(diffuse * 2), 1),
        raw.replace("1.000244140625", "nan", 1),
    ):
        with pytest.raises(ValueError):
            module.parse_original(bad, spec, prepared)
    with pytest.raises(ValueError):
        module.normalize({**spec, "arbitrary_uvspec_input": "include /etc/shadow"})
    with pytest.raises(ValueError):
        module.normalize({**spec, "relative_tolerance": 0.1})


def test_native_decimal_wavelength_resolution_and_direct_energy_bounds_are_not_weakened():
    module = adapter()
    spec = fixture()
    changed = copy.deepcopy(spec)
    changed["wavelengths"][0]["value"] = 280.0001
    with pytest.raises(ValueError):
        module.normalize(changed)
    changed["wavelengths"][0]["value"] = 280.001
    assert module.normalize(changed)["wavelengths_nm"][0] == pytest.approx(280.001)
    prepared = module.normalize(spec)
    rows = [
        " ".join(map(str, [wl, toa * 2, 0, 0, *([0] * 2048)]))
        for wl, toa in zip(
            prepared["wavelengths_nm"], prepared["toa_irradiance_w_m2_nm"]
        )
    ]
    with pytest.raises(ValueError):
        module.parse_original("\n".join(rows), spec, prepared)
