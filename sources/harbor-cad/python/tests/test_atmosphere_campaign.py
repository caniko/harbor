"""Atmospheric prerequisite assessments are reconstructed from retained originals."""

import hashlib
import importlib.util
from pathlib import Path

import pytest


def test_atmospheric_refinement_flags_cannot_hide_changed_originals_or_error(
    monkeypatch, tmp_path
):
    scripts = Path(__file__).parents[2] / "scripts"
    monkeypatch.syspath_prepend(str(scripts))
    loader = importlib.util.spec_from_file_location(
        "atmosphere_campaign", scripts / "verify_atmosphere_cpu.py"
    )
    module = importlib.util.module_from_spec(loader)
    loader.loader.exec_module(module)
    records = {}
    for name, error, bins in (
        ("clear-streams16", 0.1, None),
        ("clear-streams32", 0.01, None),
        ("clear-streams64", 0.0, 64),
        ("clear-angular16", 0.1, 16),
        ("clear-angular32", 0.01, 32),
        ("clear-wavelength4", 0.1, None),
        ("clear-wavelength2", 0.01, None),
        ("clear-wavelength1", 0.0, None),
    ):
        record = {
            "case": name,
            "prepared": {"wavelengths_nm": [300, 360]},
            "observations": {
                "direct_horizontal_w_m2_nm": [1 + error] * 2,
                "diffuse_downward_w_m2_nm": [1 + error] * 2,
                "diffuse_upward_w_m2_nm": [0] * 2,
            },
        }
        if bins:
            folder = tmp_path / name
            folder.mkdir()
            original = folder / "uvspec-original.txt"
            original.write_text(
                "\n".join(
                    " ".join(
                        map(str, [wl, 1, 1, 0, *([1 + error] * (2 * bins * bins))])
                    )
                    for wl in (300, 360)
                )
            )
            record["original_files_sha256"] = {
                original.name: hashlib.sha256(original.read_bytes()).hexdigest()
            }
        records[name] = record
    assessments = module.assess_refinements(records, tmp_path, 0.02)
    for assessment in assessments.values():
        assert assessment["passed"] and assessment[
            "relative_l2_errors_against_finest"
        ] == pytest.approx([0.1, 0.01])
    records["clear-streams32"]["observations"]["direct_horizontal_w_m2_nm"][0] = 2.0
    assert not module.assess_refinements(records, tmp_path, 0.02)["streams"]["passed"]
    original = tmp_path / "clear-angular16/uvspec-original.txt"
    original.write_text(original.read_text() + " changed")
    with pytest.raises(ValueError):
        module.assess_refinements(records, tmp_path, 0.02)
